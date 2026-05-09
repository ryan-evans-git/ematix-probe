//! S3 (and S3-compatible) Parquet adapter — built on
//! `object_store` so the same code path works against AWS S3,
//! LocalStack, MinIO, or a `LocalFileSystem` impl in tests.
//!
//! v0.1 strategy: download the whole object to a tempfile, then
//! delegate to the local-file [`ParquetAdapter`](super::parquet).
//! That trades S3 byte-range streaming for a one-line
//! implementation that reuses the existing scan-path. Streaming
//! via `parquet::arrow::async_reader::ParquetObjectReader` is the
//! eventual destination but waits on a workload that needs it.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use object_store::path::Path as ObjectPath;
use object_store::{aws::AmazonS3Builder, ObjectStore, ObjectStoreExt};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use crate::adapters::data::parquet::ParquetAdapter;
use crate::adapters::data::{AdapterError, DataAdapter};
use crate::engine::data::{ProbePlan, RunSummary};

/// Adapter for Parquet files in object storage. The "S3" in the
/// name reflects the v0.1 production target, but the underlying
/// `Arc<dyn ObjectStore>` means GCS / Azure / LocalFS Just Work
/// the same way (`object_store` does the polymorphism).
pub struct S3ParquetAdapter {
    store: Arc<dyn ObjectStore>,
    key: ObjectPath,
}

impl S3ParquetAdapter {
    /// Build an adapter pointed at `s3://<bucket>/<key>` in
    /// `region`. Use `endpoint_url` for non-AWS S3 (LocalStack,
    /// MinIO, R2, …); pass `None` for real AWS S3.
    ///
    /// Credentials follow the standard AWS resolver
    /// (env vars → credentials file → IAM role); no explicit
    /// hand-off in v0.1.
    pub fn open(
        bucket: &str,
        key: &str,
        region: &str,
        endpoint_url: Option<&str>,
    ) -> Result<Self, AdapterError> {
        let mut builder = AmazonS3Builder::from_env()
            .with_bucket_name(bucket)
            .with_region(region);
        if let Some(ep) = endpoint_url {
            builder = builder.with_endpoint(ep).with_allow_http(true);
        }
        let store = builder
            .build()
            .map_err(|e| AdapterError::Connection(format!("s3 builder: {e}")))?;
        Ok(Self {
            store: Arc::new(store),
            key: ObjectPath::from(key),
        })
    }

    /// Test-friendly constructor that takes any `ObjectStore`
    /// (typically `LocalFileSystem` for unit tests). Same code
    /// path as `open` from here on.
    pub fn from_object_store<S: AsRef<str>>(store: Arc<dyn ObjectStore>, key: S) -> Self {
        Self {
            store,
            key: ObjectPath::from(key.as_ref()),
        }
    }
}

#[async_trait]
impl DataAdapter for S3ParquetAdapter {
    async fn execute(&self, plan: &ProbePlan) -> Result<RunSummary, AdapterError> {
        // Download the object to a tempfile and delegate. Memory
        // hit is bounded by object size — fine for v0.1's "data
        // probe over a Parquet file" workloads, less fine for
        // GB+ files (Phase 4+ work).
        let bytes = self
            .store
            .get(&self.key)
            .await
            .map_err(|e| AdapterError::Connection(format!("s3 get {}: {e}", self.key)))?
            .bytes()
            .await
            .map_err(|e| AdapterError::Connection(format!("s3 read {}: {e}", self.key)))?;

        let tmp = NamedTempFile::new()
            .map_err(|e| AdapterError::Connection(format!("tempfile create: {e}")))?;
        let path: PathBuf = tmp.path().to_path_buf();
        // Write via tokio::fs to keep the awaitable surface
        // consistent; the bytes are already in memory so this is
        // really just a synchronous file write under the hood.
        let mut f = tokio::fs::File::create(&path)
            .await
            .map_err(|e| AdapterError::Connection(format!("tempfile open: {e}")))?;
        f.write_all(&bytes)
            .await
            .map_err(|e| AdapterError::Connection(format!("tempfile write: {e}")))?;
        f.flush()
            .await
            .map_err(|e| AdapterError::Connection(format!("tempfile flush: {e}")))?;
        drop(f);

        let parquet = ParquetAdapter::open(&path)?;
        let summary = parquet.execute(plan).await?;
        // Drop tmp explicitly so it survives until ParquetAdapter
        // is done reading. (NamedTempFile keeps the file alive
        // until dropped; we want that lifetime to extend past
        // execute().)
        drop(tmp);
        Ok(summary)
    }
}
