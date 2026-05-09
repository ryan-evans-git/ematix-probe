//! S-7.4 — `S3ParquetAdapter` against a real LocalStack S3
//! container. Locks in that the production code path (AmazonS3
//! object store) actually works against an S3-compatible
//! endpoint, beyond the `LocalFileSystem` test path covered by
//! `s3_parquet_adapter.rs`.
//!
//! Requires Docker. Skips locally without it like the other
//! containerized tests; runs on CI runners.

use std::fs::File;
use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use ematix_probe_core::adapters::data::s3_parquet::S3ParquetAdapter;
use ematix_probe_core::{Assertion, DataAdapter, ProbePlan, Verdict};
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use parquet::arrow::ArrowWriter;
use tempfile::TempDir;
use testcontainers_modules::localstack::LocalStack;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ImageExt;

const BUCKET: &str = "probe-test-bucket";
const KEY: &str = "users/data.parquet";

#[tokio::test]
async fn s3_adapter_runs_against_localstack() {
    // Spin LocalStack with only the S3 service for a fast start.
    let ls = LocalStack::default()
        .with_env_var("SERVICES", "s3")
        .start()
        .await
        .expect("localstack container failed to start");
    let host = ls.get_host().await.unwrap();
    let port = ls.get_host_port_ipv4(4566).await.unwrap();
    let endpoint = format!("http://{host}:{port}");

    // Build an object store pointed at LocalStack. LocalStack
    // accepts any creds; pass them explicitly so the SDK doesn't
    // attempt IMDS / metadata-service lookup (which costs 30s
    // before failing on a non-EC2 host).
    let store: Arc<dyn ObjectStore> = Arc::new(
        AmazonS3Builder::new()
            .with_bucket_name(BUCKET)
            .with_region("us-east-1")
            .with_endpoint(&endpoint)
            .with_allow_http(true)
            .with_access_key_id("test")
            .with_secret_access_key("test")
            .build()
            .expect("build LocalStack object store"),
    );

    // Create the bucket. LocalStack S3 doesn't auto-create.
    create_bucket(&endpoint, BUCKET).await;

    // Write a parquet file locally + upload its bytes.
    let dir = TempDir::new().unwrap();
    let local_path = dir.path().join("data.parquet");
    write_passing_parquet(&local_path);
    let bytes = std::fs::read(&local_path).unwrap();
    store
        .put(&ObjectPath::from(KEY), PutPayload::from(bytes))
        .await
        .expect("put object");

    // Probe the object via S3ParquetAdapter (production code path).
    let adapter = S3ParquetAdapter::from_object_store(store, KEY);
    let plan = ProbePlan {
        schema: None,
        table: "users".into(),
        assertions: vec![
            Assertion::NotNull {
                column: "email".into(),
            },
            Assertion::Unique {
                column: "id".into(),
            },
            Assertion::Between {
                column: "age".into(),
                low: 0.0,
                high: 120.0,
            },
        ],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass, "unexpected: {summary:?}");
}

/// LocalStack S3 doesn't auto-create buckets — issue a PUT
/// against the bucket root via raw reqwest. (object_store's
/// AmazonS3 client doesn't expose a CreateBucket wrapper.)
async fn create_bucket(endpoint: &str, bucket: &str) {
    let url = format!("{endpoint}/{bucket}");
    let resp = reqwest::Client::new()
        .put(&url)
        .send()
        .await
        .expect("create bucket request");
    assert!(
        resp.status().is_success() || resp.status().as_u16() == 409,
        "unexpected create-bucket status: {}",
        resp.status()
    );
}

fn write_passing_parquet(path: &std::path::Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("email", DataType::Utf8, true),
        Field::new("age", DataType::Float64, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])),
            Arc::new(StringArray::from(vec![
                Some("a@x.com"),
                Some("b@y.org"),
                Some("c@z.io"),
            ])),
            Arc::new(Float64Array::from(vec![Some(25.0), Some(40.0), Some(33.0)])),
        ],
    )
    .unwrap();
    let file = File::create(path).unwrap();
    let mut w = ArrowWriter::try_new(file, schema, None).unwrap();
    w.write(&batch).unwrap();
    w.close().unwrap();
}
