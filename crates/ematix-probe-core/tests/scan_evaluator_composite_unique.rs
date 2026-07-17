//! Scan-path composite (multi-column) uniqueness — `Assertion::UniqueGroup`.
//!
//! The tuple of key columns must be jointly unique; individual columns
//! may repeat. Batches are split so accumulator state is exercised
//! across batch boundaries. Mixed Int64 + Utf8 key columns.

use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
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

fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("email", DataType::Utf8, true),
        Field::new("user_id", DataType::Int64, true),
        Field::new("age", DataType::Float64, true),
    ]))
}

fn batch(
    schema: SchemaRef,
    emails: Vec<Option<&str>>,
    ids: Vec<Option<i64>>,
    ages: Vec<Option<f64>>,
) -> RecordBatch {
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(emails)),
            Arc::new(Int64Array::from(ids)),
            Arc::new(Float64Array::from(ages)),
        ],
    )
    .unwrap()
}

fn scanner(batches: Vec<RecordBatch>) -> VecScanner {
    VecScanner {
        schema: schema(),
        batches: batches.into_iter(),
    }
}

fn plan(assertions: Vec<Assertion>) -> ProbePlan {
    ProbePlan {
        schema: None,
        table: "memberships".into(),
        assertions,
    }
}

#[tokio::test]
async fn passes_when_tuple_unique_though_columns_repeat() {
    let s = schema();
    // email repeats (a@x twice), user_id repeats (1 twice), but the
    // (email, user_id) tuple is unique. Split across two batches.
    let mut sc = scanner(vec![
        batch(
            s.clone(),
            vec![Some("a@x"), Some("b@y")],
            vec![Some(1), Some(1)],
            vec![Some(25.0), Some(40.0)],
        ),
        batch(
            s.clone(),
            vec![Some("a@x"), Some("b@y")],
            vec![Some(2), Some(2)],
            vec![Some(33.0), Some(50.0)],
        ),
    ]);
    let p = plan(vec![Assertion::UniqueGroup {
        columns: vec!["email".into(), "user_id".into()],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass, "{summary:?}");
}

#[tokio::test]
async fn fails_on_duplicate_tuple_across_batches() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(
            s.clone(),
            vec![Some("a@x")],
            vec![Some(1)],
            vec![Some(25.0)],
        ),
        // Same (a@x, 1) tuple again in a later batch.
        batch(
            s.clone(),
            vec![Some("a@x")],
            vec![Some(1)],
            vec![Some(30.0)],
        ),
    ]);
    let p = plan(vec![Assertion::UniqueGroup {
        columns: vec!["email".into(), "user_id".into()],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    assert_eq!(summary.assertions[0].verdict, Verdict::Fail);
}

#[tokio::test]
async fn errors_on_missing_key_column() {
    let s = schema();
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![Some("a@x")],
        vec![Some(1)],
        vec![Some(25.0)],
    )]);
    let p = plan(vec![Assertion::UniqueGroup {
        columns: vec!["email".into(), "nope".into()],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.assertions[0].verdict, Verdict::Error);
}

#[tokio::test]
async fn errors_on_unsupported_key_type() {
    let s = schema();
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![Some("a@x")],
        vec![Some(1)],
        vec![Some(25.0)],
    )]);
    // age is Float64 — not a supported key type (Int64 / Utf8 only).
    let p = plan(vec![Assertion::UniqueGroup {
        columns: vec!["email".into(), "age".into()],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.assertions[0].verdict, Verdict::Error);
}
