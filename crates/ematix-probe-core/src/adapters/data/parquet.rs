//! Local Parquet data adapter — second scan-path adapter.
//!
//! A Parquet file *is* the table; `ProbePlan::table` / `schema` are
//! ignored. The adapter opens the file once at `execute` time,
//! constructs a `ParquetRecordBatchReader`, and streams batches
//! one row group at a time through the engine's scan-path
//! evaluator.
//!
//! S-4.6 shipped this with eager-collect-into-Vec to avoid any
//! borrow-lifetime puzzles. S-5.4 drops the eager collect: the
//! parquet reader is owned (no sub-borrow on a Statement like
//! DuckDB), so it can live inside the scanner across `next_batch`
//! calls. Memory profile: O(row_group_size) instead of O(file).
//!
//! Sync I/O still wraps in `tokio::task::spawn_blocking` per
//! batch — the per-call overhead is tens of microseconds, well
//! under typical row-group read times.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use arrow::array::RecordBatchReader;
use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use parquet::arrow::arrow_reader::{ParquetRecordBatchReader, ParquetRecordBatchReaderBuilder};

use crate::adapters::data::{AdapterError, DataAdapter};
use crate::engine::data::{ProbePlan, RunSummary};
use crate::engine::scan::{evaluate, ArrowBatch, Scanner};

/// Local Parquet adapter. Holds the file path; per-execute reads
/// open the file fresh (no persistent handle).
pub struct ParquetAdapter {
    path: PathBuf,
}

impl ParquetAdapter {
    /// Validate that `path` exists and is a Parquet file by
    /// constructing the reader builder once. Cheap (reads only
    /// the footer) and catches missing/corrupt files before the
    /// first execute.
    pub fn open(path: &Path) -> Result<Self, AdapterError> {
        let file = File::open(path).map_err(|e| {
            AdapterError::Connection(format!("parquet open {}: {e}", path.display()))
        })?;
        let _builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
            AdapterError::Query(format!("parquet builder for {}: {e}", path.display()))
        })?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

#[async_trait]
impl DataAdapter for ParquetAdapter {
    async fn execute(&self, plan: &ProbePlan) -> Result<RunSummary, AdapterError> {
        let path = self.path.clone();

        // Open the file + build the reader once on the blocking
        // pool. The reader is `Send` so it can be moved into the
        // scanner that next_batch will pull from.
        let (schema, reader) = tokio::task::spawn_blocking(
            move || -> Result<(SchemaRef, ParquetRecordBatchReader), AdapterError> {
                let file = File::open(&path).map_err(|e| {
                    AdapterError::Connection(format!("parquet open {}: {e}", path.display()))
                })?;
                let reader = ParquetRecordBatchReaderBuilder::try_new(file)
                    .map_err(|e| AdapterError::Query(format!("parquet builder: {e}")))?
                    .build()
                    .map_err(|e| AdapterError::Query(format!("parquet build: {e}")))?;
                let schema = reader.schema();
                Ok((schema, reader))
            },
        )
        .await
        .map_err(|e| AdapterError::Connection(format!("spawn_blocking join: {e}")))??;

        let mut scanner = ParquetScanner {
            schema,
            reader: Arc::new(Mutex::new(Some(reader))),
        };
        evaluate(plan, &mut scanner).await
    }
}

/// Streaming Parquet scanner: one row group per `next_batch` call.
///
/// The `Arc<Mutex<Option<...>>>` wrap looks heavyweight but earns
/// its keep:
/// - `Arc<Mutex<...>>` lets the closure passed into `spawn_blocking`
///   own a clone of the reader handle while the scanner keeps its
///   own (no `&mut self` lifetime puzzle inside the spawned task).
/// - `Option<...>` lets the closure `take()` the reader, advance
///   it, then put it back. Without it we'd need to hold the lock
///   across `await`, which `tokio::sync::Mutex` would handle but
///   `std::sync::Mutex` won't.
struct ParquetScanner {
    schema: SchemaRef,
    reader: Arc<Mutex<Option<ParquetRecordBatchReader>>>,
}

#[async_trait]
impl Scanner for ParquetScanner {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    async fn next_batch(&mut self) -> Result<Option<ArrowBatch>, AdapterError> {
        let reader = self.reader.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<ArrowBatch>, AdapterError> {
            let mut guard = reader
                .lock()
                .map_err(|_| AdapterError::Connection("parquet reader mutex poisoned".into()))?;
            let r = match guard.as_mut() {
                Some(r) => r,
                None => return Ok(None),
            };
            match r.next() {
                None => {
                    *guard = None;
                    Ok(None)
                }
                Some(Ok(batch)) => Ok(Some(batch)),
                Some(Err(e)) => Err(AdapterError::Query(format!("parquet read: {e}"))),
            }
        })
        .await
        .map_err(|e| AdapterError::Connection(format!("spawn_blocking join: {e}")))?
    }
}
