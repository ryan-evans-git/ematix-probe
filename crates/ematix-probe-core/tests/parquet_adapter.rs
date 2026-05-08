//! S-4.6 — `ParquetAdapter`: scan-path adapter for local Parquet
//! files. Tests write a small Parquet file to a tempdir, point the
//! adapter at it, and assert on the resulting RunSummary.

use std::fs::File;
use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use ematix_probe_core::adapters::data::parquet::ParquetAdapter;
use ematix_probe_core::{Assertion, DataAdapter, ProbePlan, Verdict};
use parquet::arrow::ArrowWriter;
use tempfile::TempDir;

fn write_parquet(path: &std::path::Path, batches: Vec<RecordBatch>) {
    let schema = batches.first().expect("at least one batch").schema();
    let file = File::create(path).expect("create parquet file");
    let mut writer = ArrowWriter::try_new(file, schema, None).expect("ArrowWriter");
    for b in batches {
        writer.write(&b).expect("write batch");
    }
    writer.close().expect("close writer");
}

fn user_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("email", DataType::Utf8, true),
        Field::new("age", DataType::Float64, true),
    ]))
}

fn user_batch(
    schema: Arc<Schema>,
    ids: Vec<i64>,
    emails: Vec<Option<&str>>,
    ages: Vec<Option<f64>>,
) -> RecordBatch {
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(ids)),
            Arc::new(StringArray::from(emails)),
            Arc::new(Float64Array::from(ages)),
        ],
    )
    .unwrap()
}

fn plan(assertions: Vec<Assertion>) -> ProbePlan {
    ProbePlan {
        // table is informational only for parquet — the adapter
        // ignores it (a Parquet file is the table).
        schema: None,
        table: "users".into(),
        assertions,
    }
}

#[tokio::test]
async fn parquet_runs_passing_probe() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("users.parquet");
    let s = user_schema();
    write_parquet(
        &path,
        vec![user_batch(
            s.clone(),
            vec![1, 2, 3],
            vec![Some("a@x.com"), Some("b@y.org"), Some("c@z.io")],
            vec![Some(25.0), Some(40.0), Some(33.0)],
        )],
    );

    let a = ParquetAdapter::open(&path).expect("open parquet");
    let p = plan(vec![
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
    ]);
    let summary = a.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass, "unexpected: {summary:?}");
}

#[tokio::test]
async fn parquet_surfaces_failures() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("dirty.parquet");
    let s = user_schema();
    write_parquet(
        &path,
        vec![
            user_batch(
                s.clone(),
                vec![1, 2],
                vec![Some("a@x.com"), None],
                vec![Some(25.0), Some(40.0)],
            ),
            user_batch(
                s.clone(),
                vec![1, 4],
                vec![Some("c@z.io"), Some("d@w.co")],
                vec![Some(200.0), Some(50.0)],
            ),
        ],
    );

    let a = ParquetAdapter::open(&path).expect("open");
    let p = plan(vec![
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
    ]);
    let summary = a.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    assert!(summary
        .assertions
        .iter()
        .all(|r| r.verdict == Verdict::Fail));
}

#[tokio::test]
async fn parquet_missing_file_yields_open_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("does-not-exist.parquet");
    let result = ParquetAdapter::open(&path);
    assert!(result.is_err(), "missing file should error; got Ok");
    drop(result);
}
