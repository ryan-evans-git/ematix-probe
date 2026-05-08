//! S-4.4 — scan-path evaluator for `row_count` and `freshness`.
//!
//! row_count is straightforward: a running counter over every
//! batch. freshness tracks the MAX of a Timestamp column across
//! batches, then compares the gap to system time at finalize.
//!
//! The "now" the evaluator uses is system time. Tests build their
//! seed timestamps relative to system time to stay deterministic
//! across slow CI runners (a few seconds of clock drift is fine
//! when the threshold is hours).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
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
        Field::new("id", DataType::Int64, false),
        Field::new(
            "updated_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
    ]))
}

fn now_micros() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

fn batch(s: SchemaRef, ids: Vec<i64>, ts_micros: Vec<Option<i64>>) -> RecordBatch {
    RecordBatch::try_new(
        s,
        vec![
            Arc::new(Int64Array::from(ids)),
            Arc::new(TimestampMicrosecondArray::from(ts_micros)),
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
        table: "events".into(),
        assertions,
    }
}

#[tokio::test]
async fn row_count_passes_within_inclusive_bounds() {
    let s = schema();
    let mut sc = scanner(vec![
        batch(s.clone(), vec![1, 2, 3], vec![None, None, None]),
        batch(s.clone(), vec![4, 5], vec![None, None]),
    ]);
    let p = plan(vec![Assertion::RowCount {
        low: Some(2),
        high: Some(10),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn row_count_fails_when_below_low() {
    let s = schema();
    let mut sc = scanner(vec![batch(s.clone(), vec![1], vec![None])]);
    let p = plan(vec![Assertion::RowCount {
        low: Some(2),
        high: None,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("msg");
    assert!(
        msg.contains("1") && msg.contains("2"),
        "msg should mention actual count + threshold: {msg:?}"
    );
}

#[tokio::test]
async fn row_count_fails_when_above_high() {
    let s = schema();
    let mut sc = scanner(vec![batch(s.clone(), vec![1, 2, 3, 4, 5], vec![None; 5])]);
    let p = plan(vec![Assertion::RowCount {
        low: None,
        high: Some(2),
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[tokio::test]
async fn row_count_both_bounds_none_yields_error() {
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::RowCount {
        low: None,
        high: None,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Error);
}

#[tokio::test]
async fn row_count_empty_table() {
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::RowCount {
        low: Some(1),
        high: None,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[tokio::test]
async fn freshness_passes_when_max_recent() {
    let s = schema();
    let now = now_micros();
    let one_hour_ago = now - 3600 * 1_000_000;
    let mut sc = scanner(vec![batch(
        s.clone(),
        vec![1, 2],
        vec![Some(one_hour_ago - 60_000_000), Some(one_hour_ago)],
    )]);
    let p = plan(vec![Assertion::Freshness {
        column: "updated_at".into(),
        within_seconds: 24 * 3600,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_fails_when_max_too_old() {
    let s = schema();
    let now = now_micros();
    let two_days_ago = now - 2 * 24 * 3600 * 1_000_000;
    let mut sc = scanner(vec![batch(s.clone(), vec![1], vec![Some(two_days_ago)])]);
    let p = plan(vec![Assertion::Freshness {
        column: "updated_at".into(),
        within_seconds: 24 * 3600,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("msg");
    assert!(
        msg.contains("updated_at"),
        "msg should reference column: {msg:?}"
    );
}

#[tokio::test]
async fn freshness_uses_max_across_batches() {
    let s = schema();
    let now = now_micros();
    let two_days_ago = now - 2 * 24 * 3600 * 1_000_000;
    let one_minute_ago = now - 60 * 1_000_000;
    // Older value first; the recent one in the second batch should
    // win because freshness asks about MAX (the most recent value).
    let mut sc = scanner(vec![
        batch(s.clone(), vec![1], vec![Some(two_days_ago)]),
        batch(s.clone(), vec![2], vec![Some(one_minute_ago)]),
    ]);
    let p = plan(vec![Assertion::Freshness {
        column: "updated_at".into(),
        within_seconds: 24 * 3600,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_empty_table_fails() {
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::Freshness {
        column: "updated_at".into(),
        within_seconds: 24 * 3600,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("msg");
    assert!(
        msg.to_lowercase().contains("no rows") || msg.to_lowercase().contains("empty"),
        "msg should mention empty/no-rows: {msg:?}"
    );
}

#[tokio::test]
async fn freshness_negative_within_seconds_yields_error() {
    let mut sc = scanner(vec![]);
    let p = plan(vec![Assertion::Freshness {
        column: "updated_at".into(),
        within_seconds: -1,
    }]);
    let summary = evaluate(&p, &mut sc).await.expect("evaluate");
    assert_eq!(summary.verdict, Verdict::Error);
}
