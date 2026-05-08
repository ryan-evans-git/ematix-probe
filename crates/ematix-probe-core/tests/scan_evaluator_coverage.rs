//! Coverage-completion tests for `engine::scan`.
//!
//! The S-4.2..S-4.4 RED tests cover the *behavioral* contract;
//! these fill in the long tail of branches that the contract tests
//! don't exercise — every Arrow numeric type for `between`, every
//! `TimeUnit` for `freshness`, missing-column errors per
//! assertion variant, and a few "unsupported Arrow type" paths
//! (unique/regex/enum on the wrong column type).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::{
    Date32Array, Float32Array, Int16Array, Int32Array, Int8Array, RecordBatch, StringArray,
    TimestampMillisecondArray, TimestampNanosecondArray, TimestampSecondArray, UInt32Array,
    UInt64Array, UInt8Array,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
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

fn one_col_batch(
    name: &str,
    dtype: DataType,
    arr: arrow::array::ArrayRef,
) -> (SchemaRef, RecordBatch) {
    let schema = Arc::new(Schema::new(vec![Field::new(name, dtype, true)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![arr]).unwrap();
    (schema, batch)
}

async fn run(
    plan: ProbePlan,
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
) -> ematix_probe_core::RunSummary {
    let mut sc = VecScanner {
        schema,
        batches: batches.into_iter(),
    };
    evaluate(&plan, &mut sc).await.expect("evaluate")
}

fn p(a: Assertion) -> ProbePlan {
    ProbePlan {
        schema: None,
        table: "t".into(),
        assertions: vec![a],
    }
}

// ---- between: every supported numeric type ----

#[tokio::test]
async fn between_handles_all_signed_integer_types() {
    for (name, dtype, arr) in [
        (
            "i8",
            DataType::Int8,
            Arc::new(Int8Array::from(vec![Some(5), Some(-1)])) as arrow::array::ArrayRef,
        ),
        (
            "i16",
            DataType::Int16,
            Arc::new(Int16Array::from(vec![Some(5), Some(-1)])),
        ),
        (
            "i32",
            DataType::Int32,
            Arc::new(Int32Array::from(vec![Some(5), Some(-1)])),
        ),
    ] {
        let (s, b) = one_col_batch(name, dtype, arr);
        // -1 is below 0, so 1 row out of range
        let summary = run(
            p(Assertion::Between {
                column: name.into(),
                low: 0.0,
                high: 100.0,
            }),
            s,
            vec![b],
        )
        .await;
        assert_eq!(summary.verdict, Verdict::Fail, "{name} should fail");
    }
}

#[tokio::test]
async fn between_handles_all_unsigned_integer_types() {
    for (name, dtype, arr) in [
        (
            "u8",
            DataType::UInt8,
            Arc::new(UInt8Array::from(vec![Some(200u8), Some(50u8)])) as arrow::array::ArrayRef,
        ),
        (
            "u32",
            DataType::UInt32,
            Arc::new(UInt32Array::from(vec![Some(200u32), Some(50u32)])),
        ),
        (
            "u64",
            DataType::UInt64,
            Arc::new(UInt64Array::from(vec![Some(200u64), Some(50u64)])),
        ),
    ] {
        let (s, b) = one_col_batch(name, dtype, arr);
        // 200 is above 100, so 1 row out of range
        let summary = run(
            p(Assertion::Between {
                column: name.into(),
                low: 0.0,
                high: 100.0,
            }),
            s,
            vec![b],
        )
        .await;
        assert_eq!(summary.verdict, Verdict::Fail, "{name} should fail");
    }
}

#[tokio::test]
async fn between_handles_float32() {
    let (s, b) = one_col_batch(
        "f32",
        DataType::Float32,
        Arc::new(Float32Array::from(vec![Some(50.0f32), Some(150.0f32)])),
    );
    let summary = run(
        p(Assertion::Between {
            column: "f32".into(),
            low: 0.0,
            high: 100.0,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[tokio::test]
async fn between_unsupported_type_promotes_to_error() {
    // Date32 isn't in the numeric supported set — should produce
    // Error verdict mid-stream when count_oob returns Err.
    let (s, b) = one_col_batch(
        "d",
        DataType::Date32,
        Arc::new(Date32Array::from(vec![Some(0), Some(100)])),
    );
    let summary = run(
        p(Assertion::Between {
            column: "d".into(),
            low: 0.0,
            high: 100.0,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
    let msg = summary.assertions[0].message.as_ref().unwrap();
    assert!(msg.to_lowercase().contains("unsupported"), "msg: {msg:?}");
}

// ---- unique: Utf8 + missing-column + unsupported-type paths ----

#[tokio::test]
async fn unique_on_utf8_column_detects_duplicate() {
    let (s, b) = one_col_batch(
        "name",
        DataType::Utf8,
        Arc::new(StringArray::from(vec![Some("a"), Some("b"), Some("a")])),
    );
    let summary = run(
        p(Assertion::Unique {
            column: "name".into(),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[tokio::test]
async fn unique_missing_column_yields_error() {
    let (s, b) = one_col_batch(
        "x",
        DataType::Int64,
        Arc::new(arrow::array::Int64Array::from(vec![Some(1)])),
    );
    let summary = run(
        p(Assertion::Unique {
            column: "nope".into(),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

#[tokio::test]
async fn unique_on_unsupported_type_yields_error() {
    let (s, b) = one_col_batch(
        "f",
        DataType::Float64,
        Arc::new(arrow::array::Float64Array::from(vec![Some(1.0), Some(1.0)])),
    );
    let summary = run(p(Assertion::Unique { column: "f".into() }), s, vec![b]).await;
    assert_eq!(summary.verdict, Verdict::Error);
}

// ---- regex / enum: missing-column + wrong-type paths ----

#[tokio::test]
async fn regex_missing_column_yields_error() {
    let (s, b) = one_col_batch(
        "x",
        DataType::Utf8,
        Arc::new(StringArray::from(vec![Some("a")])),
    );
    let summary = run(
        p(Assertion::Regex {
            column: "nope".into(),
            pattern: ".*".into(),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

#[tokio::test]
async fn regex_on_non_string_column_yields_error() {
    let (s, b) = one_col_batch(
        "n",
        DataType::Int64,
        Arc::new(arrow::array::Int64Array::from(vec![Some(1)])),
    );
    let summary = run(
        p(Assertion::Regex {
            column: "n".into(),
            pattern: ".*".into(),
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

#[tokio::test]
async fn enum_missing_column_yields_error() {
    let (s, b) = one_col_batch(
        "x",
        DataType::Utf8,
        Arc::new(StringArray::from(vec![Some("a")])),
    );
    let summary = run(
        p(Assertion::Enum {
            column: "nope".into(),
            allowed: vec!["a".into()],
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

#[tokio::test]
async fn enum_on_non_string_column_yields_error() {
    let (s, b) = one_col_batch(
        "n",
        DataType::Int64,
        Arc::new(arrow::array::Int64Array::from(vec![Some(1)])),
    );
    let summary = run(
        p(Assertion::Enum {
            column: "n".into(),
            allowed: vec!["a".into()],
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

// ---- freshness: every TimeUnit + missing-column / wrong-type ----

fn now_units(unit_per_sec: i64) -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    secs * unit_per_sec
}

#[tokio::test]
async fn freshness_handles_timestamp_second() {
    let now = now_units(1);
    let (s, b) = one_col_batch(
        "t",
        DataType::Timestamp(TimeUnit::Second, None),
        Arc::new(TimestampSecondArray::from(vec![Some(now - 60)])),
    );
    let summary = run(
        p(Assertion::Freshness {
            column: "t".into(),
            within_seconds: 24 * 3600,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_handles_timestamp_millisecond() {
    let now = now_units(1_000);
    let (s, b) = one_col_batch(
        "t",
        DataType::Timestamp(TimeUnit::Millisecond, None),
        Arc::new(TimestampMillisecondArray::from(vec![Some(now - 60_000)])),
    );
    let summary = run(
        p(Assertion::Freshness {
            column: "t".into(),
            within_seconds: 24 * 3600,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_handles_timestamp_nanosecond() {
    let now = now_units(1_000_000_000);
    let (s, b) = one_col_batch(
        "t",
        DataType::Timestamp(TimeUnit::Nanosecond, None),
        Arc::new(TimestampNanosecondArray::from(vec![Some(
            now - 60_000_000_000,
        )])),
    );
    let summary = run(
        p(Assertion::Freshness {
            column: "t".into(),
            within_seconds: 24 * 3600,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_missing_column_yields_error() {
    let (s, b) = one_col_batch(
        "x",
        DataType::Int64,
        Arc::new(arrow::array::Int64Array::from(vec![Some(1)])),
    );
    let summary = run(
        p(Assertion::Freshness {
            column: "nope".into(),
            within_seconds: 60,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

#[tokio::test]
async fn freshness_on_non_timestamp_column_yields_error() {
    let (s, b) = one_col_batch(
        "n",
        DataType::Int64,
        Arc::new(arrow::array::Int64Array::from(vec![Some(1)])),
    );
    let summary = run(
        p(Assertion::Freshness {
            column: "n".into(),
            within_seconds: 60,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}

// ---- between: missing-column path ----

#[tokio::test]
async fn between_missing_column_yields_error() {
    let (s, b) = one_col_batch(
        "x",
        DataType::Float64,
        Arc::new(arrow::array::Float64Array::from(vec![Some(1.0)])),
    );
    let summary = run(
        p(Assertion::Between {
            column: "nope".into(),
            low: 0.0,
            high: 1.0,
        }),
        s,
        vec![b],
    )
    .await;
    assert_eq!(summary.verdict, Verdict::Error);
}
