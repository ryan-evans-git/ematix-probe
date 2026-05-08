//! S-6.1 — `S3ParquetAdapter` tests via `object_store`'s
//! `LocalFileSystem` impl.
//!
//! `S3ParquetAdapter::open(bucket, key, region)` builds an
//! `AmazonS3` store under the hood — but for tests, the
//! `from_object_store(store, key)` constructor lets us point at a
//! `LocalFileSystem` containing a parquet file. Same code path
//! through the trait object; no LocalStack needed.

use std::fs::File;
use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use ematix_probe_core::adapters::data::s3_parquet::S3ParquetAdapter;
use ematix_probe_core::{Assertion, DataAdapter, ProbePlan, Verdict};
use object_store::local::LocalFileSystem;
use object_store::ObjectStore;
use parquet::arrow::ArrowWriter;
use tempfile::TempDir;

fn write_parquet(path: &std::path::Path) -> Arc<Schema> {
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
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    schema
}

fn plan(assertions: Vec<Assertion>) -> ProbePlan {
    ProbePlan {
        schema: None,
        table: "users".into(),
        assertions,
    }
}

#[tokio::test]
async fn s3_adapter_runs_passing_probe_via_local_object_store() {
    let dir = TempDir::new().unwrap();
    let parquet_name = "users.parquet";
    let parquet_path = dir.path().join(parquet_name);
    write_parquet(&parquet_path);

    let store: Arc<dyn ObjectStore> = Arc::new(LocalFileSystem::new_with_prefix(dir.path()).unwrap());
    let adapter = S3ParquetAdapter::from_object_store(store, parquet_name);

    let p = plan(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique { column: "id".into() },
        Assertion::Between {
            column: "age".into(),
            low: 0.0,
            high: 120.0,
        },
    ]);
    let summary = adapter.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass, "unexpected: {summary:?}");
}

#[tokio::test]
async fn s3_adapter_surfaces_failures() {
    let dir = TempDir::new().unwrap();
    let parquet_name = "dirty.parquet";
    let parquet_path = dir.path().join(parquet_name);

    // Reuse write_parquet but with violations.
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("email", DataType::Utf8, true),
        Field::new("age", DataType::Float64, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 1])),
            Arc::new(StringArray::from(vec![None, Some("b@y.org")])),
            Arc::new(Float64Array::from(vec![Some(200.0), Some(40.0)])),
        ],
    )
    .unwrap();
    let file = File::create(&parquet_path).unwrap();
    let mut w = ArrowWriter::try_new(file, schema, None).unwrap();
    w.write(&batch).unwrap();
    w.close().unwrap();

    let store: Arc<dyn ObjectStore> = Arc::new(LocalFileSystem::new_with_prefix(dir.path()).unwrap());
    let adapter = S3ParquetAdapter::from_object_store(store, parquet_name);

    let p = plan(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique { column: "id".into() },
        Assertion::Between {
            column: "age".into(),
            low: 0.0,
            high: 120.0,
        },
    ]);
    let summary = adapter.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    assert!(summary
        .assertions
        .iter()
        .all(|r| r.verdict == Verdict::Fail));
}

#[tokio::test]
async fn s3_adapter_missing_object_yields_error() {
    let dir = TempDir::new().unwrap();
    let store: Arc<dyn ObjectStore> = Arc::new(LocalFileSystem::new_with_prefix(dir.path()).unwrap());
    let adapter = S3ParquetAdapter::from_object_store(store, "does-not-exist.parquet");
    let p = plan(vec![Assertion::NotNull {
        column: "id".into(),
    }]);
    let result = adapter.execute(&p).await;
    assert!(
        result.is_err(),
        "missing object should be a fetch error; got Ok"
    );
    drop(result);
}
