//! S-4.2 — scan-path evaluator for `not_null`, `unique`, `between`.
//!
//! Mirrors the Postgres adapter's behavior contract from Sprint 2 —
//! same Verdict / message shape — but produced by accumulating over
//! Arrow batches instead of pushdown SQL counts. These three tests
//! cover the happy path (all-pass) and one fail per assertion.
//!
//! Each test builds a multi-batch `VecScanner` so the accumulator
//! correctness is exercised across batch boundaries. A single-batch
//! version would not tell us whether state crosses batches.

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
        table: "users".into(),
        assertions,
    }
}

#[tokio::test]
async fn passes_when_all_three_assertions_satisfied() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(
            s.clone(),
            vec![Some("a@x"), Some("b@y")],
            vec![Some(1), Some(2)],
            vec![Some(25.0), Some(40.0)],
        ),
        batch(
            s.clone(),
            vec![Some("c@z"), Some("d@w")],
            vec![Some(3), Some(4)],
            vec![Some(33.0), Some(50.0)],
        ),
    ]);
    let p = plan(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique {
            column: "user_id".into(),
        },
        Assertion::Between {
            column: "age".into(),
            low: 0.0,
            high: 120.0,
        },
    ]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
    assert_eq!(summary.assertions.len(), 3);
    assert!(summary
        .assertions
        .iter()
        .all(|a| a.verdict == Verdict::Pass));
}

#[tokio::test]
async fn not_null_fails_when_null_in_later_batch() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(
            s.clone(),
            vec![Some("a@x"), Some("b@y")],
            vec![Some(1), Some(2)],
            vec![Some(25.0), Some(40.0)],
        ),
        // NULL email in second batch — accumulator must catch it.
        batch(
            s.clone(),
            vec![None, Some("d@w")],
            vec![Some(3), Some(4)],
            vec![Some(33.0), Some(50.0)],
        ),
    ]);
    let p = plan(vec![Assertion::NotNull {
        column: "email".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    assert_eq!(summary.assertions[0].verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("email"),
        "msg should reference column: {msg:?}"
    );
    assert!(msg.contains('1'), "msg should mention 1 NULL row: {msg:?}");
}

#[tokio::test]
async fn unique_fails_when_duplicate_spans_two_batches() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(
            s.clone(),
            vec![Some("a@x")],
            vec![Some(1)],
            vec![Some(25.0)],
        ),
        // user_id=1 already seen in batch 1 → duplicate.
        batch(
            s.clone(),
            vec![Some("b@y")],
            vec![Some(1)],
            vec![Some(30.0)],
        ),
    ]);
    let p = plan(vec![Assertion::Unique {
        column: "user_id".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("user_id"),
        "msg should reference column: {msg:?}"
    );
}

#[tokio::test]
async fn between_fails_when_value_out_of_range() {
    let s = schema();
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![Some("a@x"), Some("b@y")],
        vec![Some(1), Some(2)],
        vec![Some(25.0), Some(200.0)], // 200 > 120
    )]);
    let p = plan(vec![Assertion::Between {
        column: "age".into(),
        low: 0.0,
        high: 120.0,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(msg.contains("age"), "msg should reference column: {msg:?}");
    assert!(
        msg.contains("1"),
        "msg should mention 1 out-of-range row: {msg:?}"
    );
}

#[tokio::test]
async fn empty_scanner_passes_for_not_null_and_unique() {
    // Empty stream: no rows means no NULLs and no duplicates.
    let mut sc = scanner(vec![]);
    let p = plan(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique {
            column: "user_id".into(),
        },
    ]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn missing_column_yields_error_verdict() {
    // Plan references a column the scanner's schema doesn't have.
    // Should produce an Error result (not panic, not silently pass).
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::NotNull {
        column: "nope".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Error);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("nope"),
        "msg should reference missing col: {msg:?}"
    );
}
