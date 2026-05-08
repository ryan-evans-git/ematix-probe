//! S-4.3 — scan-path evaluator for `regex` and `enum`.
//!
//! Both checks are NULL-safe (NULLs are not counted as violations,
//! matching the Postgres adapter contract from Sprint 3 — pair
//! with `NotNull` to forbid). Tests use the same VecScanner +
//! schema fixture as `scan_evaluator_basics.rs`.

use std::sync::Arc;

use arrow::array::StringArray;
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
        Field::new("country", DataType::Utf8, true),
    ]))
}

fn batch(s: SchemaRef, emails: Vec<Option<&str>>, countries: Vec<Option<&str>>) -> RecordBatch {
    RecordBatch::try_new(
        s,
        vec![
            Arc::new(StringArray::from(emails)),
            Arc::new(StringArray::from(countries)),
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
async fn regex_passes_when_all_match() {
    let s = schema();
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![Some("a@x.com"), Some("b@y.org"), Some("c@z.io")],
        vec![Some("US"), Some("CA"), Some("MX")],
    )]);
    let p = plan(vec![Assertion::Regex {
        column: "email".into(),
        pattern: r".+@.+\..+".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn regex_fails_when_one_row_doesnt_match() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(s.clone(), vec![Some("a@x.com")], vec![Some("US")]),
        batch(
            s.clone(),
            vec![Some("not-an-email")], // fails .+@.+\..+
            vec![Some("CA")],
        ),
    ]);
    let p = plan(vec![Assertion::Regex {
        column: "email".into(),
        pattern: r".+@.+\..+".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("email"),
        "msg should reference column: {msg:?}"
    );
    assert!(msg.contains('1'), "msg should mention 1 violation: {msg:?}");
}

#[tokio::test]
async fn regex_skips_nulls() {
    let s = schema();
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![Some("a@x.com"), None, Some("c@z.io")],
        vec![Some("US"), Some("CA"), Some("MX")],
    )]);
    let p = plan(vec![Assertion::Regex {
        column: "email".into(),
        pattern: r".+@.+\..+".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn regex_invalid_pattern_yields_error() {
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::Regex {
        column: "email".into(),
        pattern: "[unclosed".into(),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Error);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.to_lowercase().contains("regex") || msg.to_lowercase().contains("pattern"),
        "msg should mention regex/pattern: {msg:?}"
    );
}

#[tokio::test]
async fn enum_passes_when_all_in_allowed() {
    let s = schema();
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![Some("a@x.com"), Some("b@y.org")],
        vec![Some("US"), Some("CA")],
    )]);
    let p = plan(vec![Assertion::Enum {
        column: "country".into(),
        allowed: vec!["US".into(), "CA".into(), "MX".into()],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn enum_fails_when_value_outside_allowed_set() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(s.clone(), vec![Some("a@x.com")], vec![Some("US")]),
        batch(s.clone(), vec![Some("b@y.org")], vec![Some("GB")]), // GB ∉ allowed
    ]);
    let p = plan(vec![Assertion::Enum {
        column: "country".into(),
        allowed: vec!["US".into(), "CA".into(), "MX".into()],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("country"),
        "msg should reference column: {msg:?}"
    );
}

#[tokio::test]
async fn enum_empty_allowed_yields_error() {
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::Enum {
        column: "country".into(),
        allowed: vec![],
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Error);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.to_lowercase().contains("empty") || msg.to_lowercase().contains("allowed"),
        "msg should mention empty/allowed: {msg:?}"
    );
}
