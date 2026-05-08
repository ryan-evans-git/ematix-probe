//! S-5.1 — `Assertion::PercentileBetween` scan-path evaluator.
//!
//! Asserts that the `p`-th percentile of a numeric column falls
//! within `[low, high]` inclusive. `p` is in `[0.0, 1.0]`; NULLs
//! are excluded from the percentile computation. Empty / all-NULL
//! columns produce `Verdict::Error` ("not enough data").
//!
//! v0.1 uses the nearest-rank method (no interpolation): for
//! `n` non-NULL values sorted ascending, percentile `p` returns
//! the value at index `floor(p * (n - 1))`. Future: t-digest for
//! streaming when memory pressure becomes a concern.

use std::sync::Arc;

use arrow::array::{Float64Array, Int64Array, RecordBatch};
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

fn schema_f64() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new("v", DataType::Float64, true)]))
}

fn batch_f64(s: SchemaRef, values: Vec<Option<f64>>) -> RecordBatch {
    RecordBatch::try_new(s, vec![Arc::new(Float64Array::from(values))]).unwrap()
}

fn schema_i64() -> SchemaRef {
    Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, true)]))
}

fn batch_i64(s: SchemaRef, values: Vec<Option<i64>>) -> RecordBatch {
    RecordBatch::try_new(s, vec![Arc::new(Int64Array::from(values))]).unwrap()
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
async fn p50_within_range_passes() {
    let s = schema_f64();
    // 11 values 0..10; P50 = value at floor(0.5 * 10) = index 5 = 5.0
    let values: Vec<Option<f64>> = (0..=10).map(|i| Some(i as f64)).collect();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 4.0,
            high: 6.0,
        }),
        s.clone(),
        vec![batch_f64(s, values)],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn p50_below_range_fails() {
    let s = schema_f64();
    let values: Vec<Option<f64>> = (0..=10).map(|i| Some(i as f64)).collect();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 100.0,
            high: 200.0,
        }),
        s.clone(),
        vec![batch_f64(s, values)],
    )
    .await;
    assert_eq!(v, Verdict::Fail);
}

#[tokio::test]
async fn p99_picks_near_max() {
    let s = schema_f64();
    let values: Vec<Option<f64>> = (0..=99).map(|i| Some(i as f64)).collect();
    // n=100, p=0.99 → idx = floor(0.99 * 99) = 98 → value 98.0
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.99,
            low: 97.0,
            high: 99.0,
        }),
        s.clone(),
        vec![batch_f64(s, values)],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn percentile_works_across_batches() {
    let s = schema_f64();
    // Split 0..10 across two batches; p50 still = 5.
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 4.0,
            high: 6.0,
        }),
        s.clone(),
        vec![
            batch_f64(s.clone(), (0..5).map(|i| Some(i as f64)).collect()),
            batch_f64(s, (5..=10).map(|i| Some(i as f64)).collect()),
        ],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn percentile_skips_nulls() {
    let s = schema_f64();
    // Mix of NULL and 0..4. With 5 non-NULL values, p50 = value at idx 2 = 2.0.
    let values: Vec<Option<f64>> = vec![
        None,
        Some(0.0),
        None,
        Some(1.0),
        Some(2.0),
        None,
        Some(3.0),
        Some(4.0),
    ];
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 1.5,
            high: 2.5,
        }),
        s.clone(),
        vec![batch_f64(s, values)],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn percentile_works_on_int64() {
    let s = schema_i64();
    let values: Vec<Option<i64>> = (0..=10).collect::<Vec<_>>().into_iter().map(Some).collect();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 4.0,
            high: 6.0,
        }),
        s.clone(),
        vec![batch_i64(s, values)],
    )
    .await;
    assert_eq!(v, Verdict::Pass);
}

#[tokio::test]
async fn empty_column_yields_error() {
    let s = schema_f64();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 0.0,
            high: 100.0,
        }),
        s,
        vec![],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}

#[tokio::test]
async fn all_null_column_yields_error() {
    let s = schema_f64();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 0.0,
            high: 100.0,
        }),
        s.clone(),
        vec![batch_f64(s, vec![None, None, None])],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}

#[tokio::test]
async fn p_out_of_range_yields_error() {
    let s = schema_f64();
    for bad_p in [-0.1, 1.1, f64::NAN] {
        let v = run(
            p(Assertion::PercentileBetween {
                column: "v".into(),
                p: bad_p,
                low: 0.0,
                high: 100.0,
            }),
            s.clone(),
            vec![batch_f64(s.clone(), vec![Some(1.0)])],
        )
        .await;
        assert_eq!(v, Verdict::Error, "p={bad_p} should error");
    }
}

#[tokio::test]
async fn unsupported_column_type_yields_error() {
    use arrow::array::StringArray;
    let s = Arc::new(Schema::new(vec![Field::new("v", DataType::Utf8, true)]));
    let b = RecordBatch::try_new(
        s.clone(),
        vec![Arc::new(StringArray::from(vec![Some("x")]))],
    )
    .unwrap();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "v".into(),
            p: 0.5,
            low: 0.0,
            high: 100.0,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}

#[tokio::test]
async fn missing_column_yields_error() {
    let s = schema_f64();
    let v = run(
        p(Assertion::PercentileBetween {
            column: "nope".into(),
            p: 0.5,
            low: 0.0,
            high: 100.0,
        }),
        s.clone(),
        vec![batch_f64(s, vec![Some(1.0)])],
    )
    .await;
    assert_eq!(v, Verdict::Error);
}
