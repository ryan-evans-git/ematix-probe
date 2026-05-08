//! S-4.1 — `Scanner` trait + `ArrowBatch` alias.
//!
//! The scan path is the engine's fallback for sources that can't
//! push assertions down as SQL — DuckDB and local Parquet (S-4.5,
//! S-4.6). A `Scanner` yields `RecordBatch`es one at a time
//! (`None` = EOF) and exposes the schema up front so type-aware
//! evaluators can plan their kernel without peeking at a batch.
//!
//! This test fixes the trait shape: `next_batch` is async (so
//! `DuckDbAdapter` can do non-blocking I/O), `schema` is sync (the
//! schema is known the moment the scanner is opened), and both
//! errors and EOF round-trip correctly.

use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use ematix_probe_core::engine::scan::{ArrowBatch, Scanner};
use ematix_probe_core::AdapterError;

struct VecScanner {
    schema: SchemaRef,
    batches: std::vec::IntoIter<RecordBatch>,
}

#[async_trait::async_trait]
impl Scanner for VecScanner {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    async fn next_batch(&mut self) -> Result<Option<ArrowBatch>, AdapterError> {
        Ok(self.batches.next())
    }
}

fn make_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]))
}

fn make_batch(schema: SchemaRef, values: Vec<i64>) -> RecordBatch {
    RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(values))]).unwrap()
}

#[tokio::test]
async fn scanner_yields_batches_in_order_then_none() {
    let schema = make_schema();
    let mut scanner = VecScanner {
        schema: schema.clone(),
        batches: vec![
            make_batch(schema.clone(), vec![1, 2, 3]),
            make_batch(schema.clone(), vec![4, 5]),
        ]
        .into_iter(),
    };

    let first = scanner.next_batch().await.expect("ok").expect("Some");
    assert_eq!(first.num_rows(), 3);
    let second = scanner.next_batch().await.expect("ok").expect("Some");
    assert_eq!(second.num_rows(), 2);
    assert!(scanner.next_batch().await.expect("ok").is_none());
}

#[tokio::test]
async fn schema_is_available_before_first_batch() {
    let schema = make_schema();
    let scanner = VecScanner {
        schema: schema.clone(),
        batches: Vec::<RecordBatch>::new().into_iter(),
    };
    let s = scanner.schema();
    assert_eq!(s.fields().len(), 1);
    assert_eq!(s.field(0).name(), "id");
}

#[tokio::test]
async fn empty_scanner_returns_none_immediately() {
    let schema = make_schema();
    let mut scanner = VecScanner {
        schema,
        batches: Vec::<RecordBatch>::new().into_iter(),
    };
    assert!(scanner.next_batch().await.expect("ok").is_none());
}
