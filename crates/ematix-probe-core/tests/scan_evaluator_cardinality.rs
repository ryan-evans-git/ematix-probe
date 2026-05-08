//! S-5.2 — `Assertion::CardinalityBetween` scan-path evaluator.
//!
//! Counts distinct non-NULL values in a column and asserts the
//! count falls within `[low, high]` inclusive. Same `Option<i64>`
//! bound shape as `RowCount` (either side `None` = unbounded).
//! NULLs are not counted (matches SQL `COUNT(DISTINCT col)`).
//!
//! Supported column types: `Int64`, `Utf8` — same set as
//! `Unique`. Other numeric types defer until a real ask.

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

fn one_col<A: arrow::array::Array + 'static>(
    name: &str,
    dtype: DataType,
    arr: A,
) -> (SchemaRef, RecordBatch) {
    let schema = Arc::new(Schema::new(vec![Field::new(name, dtype, true)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(arr)]).unwrap();
    (schema, batch)
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
        table: "t".into(),
        assertions: vec![a],
    }
}

#[tokio::test]
async fn cardinality_passes_within_inclusive_bounds() {
    let (s, b) = one_col(
        "id",
        DataType::Int64,
        Int64Array::from(vec![Some(1), Some(2), Some(2), Some(3)]),
    );
    // 3 distinct values: {1, 2, 3}
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: Some(2),
            high: Some(5),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn cardinality_fails_below_low() {
    let (s, b) = one_col(
        "id",
        DataType::Int64,
        Int64Array::from(vec![Some(1), Some(1), Some(1)]),
    );
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: Some(2),
            high: None,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn cardinality_fails_above_high() {
    let (s, b) = one_col(
        "id",
        DataType::Int64,
        Int64Array::from(vec![Some(1), Some(2), Some(3), Some(4), Some(5)]),
    );
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: None,
            high: Some(3),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn cardinality_excludes_nulls() {
    let (s, b) = one_col(
        "id",
        DataType::Int64,
        Int64Array::from(vec![Some(1), None, Some(2), None]),
    );
    // 2 distinct values: {1, 2}; NULL is not counted.
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: Some(2),
            high: Some(2),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn cardinality_dedupes_across_batches() {
    let s: SchemaRef = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
    let b1 = RecordBatch::try_new(
        s.clone(),
        vec![Arc::new(Int64Array::from(vec![Some(1), Some(2)]))],
    )
    .unwrap();
    let b2 = RecordBatch::try_new(
        s.clone(),
        vec![Arc::new(Int64Array::from(vec![Some(2), Some(3)]))],
    )
    .unwrap();
    // Total 4 rows; distinct = {1, 2, 3} = 3
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: Some(3),
            high: Some(3),
        }),
        s,
        vec![b1, b2],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn cardinality_works_on_utf8() {
    let (s, b) = one_col(
        "country",
        DataType::Utf8,
        StringArray::from(vec![Some("US"), Some("CA"), Some("US")]),
    );
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "country".into(),
            low: Some(2),
            high: Some(2),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn cardinality_empty_column_is_zero() {
    let s: SchemaRef = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: Some(1),
            high: None,
        }),
        s,
        vec![],
    )
    .await;
    // 0 distinct values; "at least 1" should fail.
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn cardinality_both_bounds_none_yields_error() {
    let s: SchemaRef = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "id".into(),
            low: None,
            high: None,
        }),
        s,
        vec![],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}

#[tokio::test]
async fn cardinality_unsupported_type_yields_error() {
    use arrow::array::Float64Array;
    let (s, b) = one_col(
        "f",
        DataType::Float64,
        Float64Array::from(vec![Some(1.0)]),
    );
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "f".into(),
            low: Some(0),
            high: Some(10),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}

#[tokio::test]
async fn cardinality_missing_column_yields_error() {
    let s: SchemaRef = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
    let v = run(
        p(Assertion::CardinalityBetween {
            column: "nope".into(),
            low: Some(0),
            high: Some(10),
        }),
        s,
        vec![],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}
