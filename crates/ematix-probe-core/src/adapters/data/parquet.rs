//! Local Parquet data adapter — second scan-path adapter.
//!
//! A Parquet file *is* the table; `ProbePlan::table` / `schema` are
//! ignored. The adapter opens the file, reads it through
//! `ParquetRecordBatchReaderBuilder` to get an Arrow batch
//! iterator, eager-collects into a `Vec<RecordBatch>`, and feeds
//! the engine's scan-path evaluator.
//!
//! The Parquet reader is sync, so file I/O happens inside
//! `tokio::task::spawn_blocking`. Memory trade-off matches the
//! DuckDB adapter (S-4.5): eager-collection sidesteps borrow
//! lifetimes between the reader and per-row-group state. Streaming
//! lands with the S3 Parquet work in Phase 3 where files routinely
//! exceed RAM.

use std::fs::File;
use std::path::{Path, PathBuf};

use arrow::array::RecordBatchReader;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

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

        let (schema, batches) = tokio::task::spawn_blocking(
            move || -> Result<(SchemaRef, Vec<RecordBatch>), AdapterError> {
                let file = File::open(&path).map_err(|e| {
                    AdapterError::Connection(format!("parquet open {}: {e}", path.display()))
                })?;
                let reader = ParquetRecordBatchReaderBuilder::try_new(file)
                    .map_err(|e| AdapterError::Query(format!("parquet builder: {e}")))?
                    .build()
                    .map_err(|e| AdapterError::Query(format!("parquet build: {e}")))?;
                let schema = reader.schema();
                let batches: Vec<RecordBatch> = reader
                    .collect::<Result<_, _>>()
                    .map_err(|e| AdapterError::Query(format!("parquet read: {e}")))?;
                Ok((schema, batches))
            },
        )
        .await
        .map_err(|e| AdapterError::Connection(format!("spawn_blocking join: {e}")))??;

        let mut scanner = VecScanner {
            schema,
            iter: batches.into_iter(),
        };
        evaluate(plan, &mut scanner).await
    }
}

struct VecScanner {
    schema: SchemaRef,
    iter: std::vec::IntoIter<RecordBatch>,
}

#[async_trait]
impl Scanner for VecScanner {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    async fn next_batch(&mut self) -> Result<Option<ArrowBatch>, AdapterError> {
        Ok(self.iter.next())
    }
}
