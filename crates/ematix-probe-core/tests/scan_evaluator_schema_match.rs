//! S-5.3 — `Assertion::SchemaMatch` scan-path evaluator.
//!
//! Strict equality check on the scanner's schema: same field
//! names, same Arrow `DataType`s, same order. Nullability is
//! deliberately not checked in v0.1 (DuckDB / Parquet readers
//! often surface columns as nullable even when the source data
//! has no NULLs, so a strict nullability check would false-fail).
//!
//! `SchemaMatch` only inspects the schema — it doesn't need to
//! pull a single batch. The check happens at acc-build, so an
//! empty-stream probe still produces a meaningful Verdict.
//!
//! Empty `fields` list → Error (asserts nothing).

use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use ematix_probe_core::engine::scan::{evaluate, ArrowBatch, Scanner};
use ematix_probe_core::{AdapterError, Assertion, ProbePlan, Verdict};

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

fn users_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("email", DataType::Utf8, true),
    ]))
}

fn users_batch(s: SchemaRef) -> RecordBatch {
    RecordBatch::try_new(
        s,
        vec![
            Arc::new(Int64Array::from(vec![Some(1)])),
            Arc::new(StringArray::from(vec![Some("a@x")])),
        ],
    )
    .unwrap()
}

async fn run(plan: ProbePlan, schema: SchemaRef, batches: Vec<RecordBatch>) -> Verdict {
    let mut sc = VecScanner {
        schema,
        batches: batches.into_iter(),
    };
    evaluate(&plan, &mut sc).await.expect("evaluate").verdict
}

fn p(a: Assertion) -> ProbePlan {
    ProbePlan {
        schema: None,
        table: "users".into(),
        assertions: vec![a],
    }
}

#[tokio::test]
async fn schema_match_passes_when_exact_match() {
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![
                ("id".into(), DataType::Int64),
                ("email".into(), DataType::Utf8),
            ],
        }),
        s.clone(),
        vec![users_batch(s)],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn schema_match_fails_on_different_field_name() {
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![
                ("user_id".into(), DataType::Int64), // wrong name
                ("email".into(), DataType::Utf8),
            ],
        }),
        s.clone(),
        vec![users_batch(s)],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn schema_match_fails_on_different_type() {
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![
                ("id".into(), DataType::Int32), // wrong type
                ("email".into(), DataType::Utf8),
            ],
        }),
        s.clone(),
        vec![users_batch(s)],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn schema_match_fails_on_different_field_order() {
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![
                ("email".into(), DataType::Utf8),
                ("id".into(), DataType::Int64),
            ],
        }),
        s.clone(),
        vec![users_batch(s)],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn schema_match_fails_on_missing_field() {
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![("id".into(), DataType::Int64)], // missing email
        }),
        s.clone(),
        vec![users_batch(s)],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn schema_match_fails_on_extra_field() {
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![
                ("id".into(), DataType::Int64),
                ("email".into(), DataType::Utf8),
                ("extra".into(), DataType::Int64), // not in scanner schema
            ],
        }),
        s.clone(),
        vec![users_batch(s)],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn schema_match_works_on_empty_stream() {
    // Schema is known the moment the scanner is opened — no
    // batch needed.
    let s = users_schema();
    let v = run(
        p(Assertion::SchemaMatch {
            fields: vec![
                ("id".into(), DataType::Int64),
                ("email".into(), DataType::Utf8),
            ],
        }),
        s,
        vec![],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn schema_match_empty_fields_yields_error() {
    let s = users_schema();
    let v = run(p(Assertion::SchemaMatch { fields: vec![] }), s, vec![]).await;
    assert_eq!(v, Verdict::Error);
}

#[tokio::test]
async fn schema_match_message_pinpoints_the_diff() {
    // Failure messages should be useful enough that the user can
    // figure out which field is wrong without re-running with a
    // debugger.
    let s = users_schema();
    let mut sc = VecScanner {
        schema: s.clone(),
        batches: vec![users_batch(s)].into_iter(),
    };
    let pln = p(Assertion::SchemaMatch {
        fields: vec![
            ("id".into(), DataType::Int32), // wrong: actual is Int64
            ("email".into(), DataType::Utf8),
        ],
    });
    let summary = evaluate(&pln, &mut sc).await.expect("evaluate");
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("id"),
        "msg should reference field name: {msg:?}"
    );
    assert!(
        msg.contains("Int32") && msg.contains("Int64"),
        "msg should reference both expected and actual types: {msg:?}"
    );
}
