//! Scan path: pull-based stream of Arrow `RecordBatch`es +
//! shared evaluator that runs a `ProbePlan` against any `Scanner`.
//!
//! Postgres pushes assertions down as SQL — counts come back from
//! the server and the engine never sees rows. Sources that can't
//! do that (local Parquet, in-process DuckDB, future S3 Parquet)
//! instead expose a `Scanner` and let `evaluate` stream batches
//! through per-assertion accumulators.
//!
//! Design notes:
//! - `next_batch` is `async` so DuckDB / Parquet I/O can be
//!   non-blocking under the same `tokio` runtime that drives the
//!   Postgres adapter. Empty stream → first call returns `None`.
//! - `schema` is sync because every backend knows the schema the
//!   moment the scanner is opened — no need to peek at a batch.
//! - The evaluator builds one accumulator per assertion *up front*
//!   from the schema, so missing-column / wrong-type errors surface
//!   without scanning a single row.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::{
    Array, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array, Int8Array, StringArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use arrow::datatypes::{DataType, Schema, SchemaRef, TimeUnit};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use regex::Regex;

use crate::adapters::data::AdapterError;
use crate::engine::data::{
    reduce_verdict, Assertion, AssertionResult, ProbePlan, RunSummary, Verdict,
};

/// Canonical Arrow batch type used by the scan path.
pub type ArrowBatch = RecordBatch;

/// Pull-based source of Arrow batches.
#[async_trait]
pub trait Scanner: Send {
    fn schema(&self) -> SchemaRef;
    async fn next_batch(&mut self) -> Result<Option<ArrowBatch>, AdapterError>;
}

/// Run `plan` against `scanner` and produce a `RunSummary`.
///
/// One accumulator per assertion is built from the scanner's schema
/// before the first batch is pulled — that's where missing-column
/// and unsupported-type errors are surfaced. Each batch is then
/// fed through every accumulator; finalize at end of stream.
pub async fn evaluate(
    plan: &ProbePlan,
    scanner: &mut dyn Scanner,
) -> Result<RunSummary, AdapterError> {
    let schema = scanner.schema();
    let mut accs: Vec<Acc> = plan
        .assertions
        .iter()
        .map(|a| Acc::build(a, &schema))
        .collect();

    while let Some(batch) = scanner.next_batch().await? {
        for acc in &mut accs {
            acc.update(&batch);
        }
    }

    let results: Vec<AssertionResult> = accs
        .into_iter()
        .enumerate()
        .map(|(i, a)| a.finalize(i))
        .collect();
    Ok(RunSummary {
        verdict: reduce_verdict(&results),
        assertions: results,
    })
}

/// One per assertion. Each variant carries whatever state the
/// assertion needs to accumulate across batches; `Error` is a
/// terminal variant for setup failures (missing column, etc) so
/// the rest of the run still proceeds.
enum Acc {
    NotNull {
        column: String,
        col_idx: usize,
        null_count: u64,
    },
    UniqueI64 {
        column: String,
        col_idx: usize,
        seen: HashSet<i64>,
        dup_count: u64,
    },
    UniqueStr {
        column: String,
        col_idx: usize,
        seen: HashSet<String>,
        dup_count: u64,
    },
    Between {
        column: String,
        col_idx: usize,
        low: f64,
        high: f64,
        oob_count: u64,
    },
    Regex {
        column: String,
        col_idx: usize,
        re: Regex,
        pattern: String,
        miss_count: u64,
    },
    Enum {
        column: String,
        col_idx: usize,
        allowed: HashSet<String>,
        allowed_len: usize,
        miss_count: u64,
    },
    RowCount {
        low: Option<i64>,
        high: Option<i64>,
        count: i64,
    },
    Freshness {
        column: String,
        col_idx: usize,
        unit: TimeUnit,
        within_seconds: i64,
        /// Largest timestamp seen so far, in `unit`'s native units.
        /// `None` until the first non-NULL value is observed.
        max_value: Option<i64>,
    },
    PercentileBetween {
        column: String,
        col_idx: usize,
        p: f64,
        low: f64,
        high: f64,
        /// Buffered non-NULL values across all batches. Sorted at
        /// finalize time. v0.1 trade-off: O(n) memory in service of
        /// implementation simplicity. Streaming approximations
        /// (t-digest etc.) deferred until a real workload pushes
        /// through enough rows to matter.
        values: Vec<f64>,
    },
    /// Setup-time failure (missing column, unsupported type, or
    /// assertion not yet implemented for the scan path). Skips
    /// `update` and finalizes to `Verdict::Error`.
    Error { message: String },
}

impl Acc {
    fn build(assertion: &Assertion, schema: &Schema) -> Acc {
        match assertion {
            Assertion::NotNull { column } => match column_index(schema, column) {
                Ok(col_idx) => Acc::NotNull {
                    column: column.clone(),
                    col_idx,
                    null_count: 0,
                },
                Err(msg) => Acc::Error { message: msg },
            },
            Assertion::Unique { column } => match column_index(schema, column) {
                Ok(col_idx) => match schema.field(col_idx).data_type() {
                    DataType::Int64 => Acc::UniqueI64 {
                        column: column.clone(),
                        col_idx,
                        seen: HashSet::new(),
                        dup_count: 0,
                    },
                    DataType::Utf8 => Acc::UniqueStr {
                        column: column.clone(),
                        col_idx,
                        seen: HashSet::new(),
                        dup_count: 0,
                    },
                    other => Acc::Error {
                        message: format!(
                            "scan-path unique on column {column:?}: unsupported Arrow type \
                             {other:?} (supported: Int64, Utf8)"
                        ),
                    },
                },
                Err(msg) => Acc::Error { message: msg },
            },
            Assertion::Between { column, low, high } => match column_index(schema, column) {
                Ok(col_idx) => Acc::Between {
                    column: column.clone(),
                    col_idx,
                    low: *low,
                    high: *high,
                    oob_count: 0,
                },
                Err(msg) => Acc::Error { message: msg },
            },
            Assertion::Regex { column, pattern } => match column_index(schema, column) {
                Ok(col_idx) => match schema.field(col_idx).data_type() {
                    DataType::Utf8 => match Regex::new(pattern) {
                        Ok(re) => Acc::Regex {
                            column: column.clone(),
                            col_idx,
                            re,
                            pattern: pattern.clone(),
                            miss_count: 0,
                        },
                        Err(e) => Acc::Error {
                            message: format!(
                                "scan-path regex on column {column:?}: invalid regex pattern \
                                 {pattern:?}: {e}"
                            ),
                        },
                    },
                    other => Acc::Error {
                        message: format!(
                            "scan-path regex on column {column:?}: unsupported Arrow type \
                             {other:?} (supported: Utf8)"
                        ),
                    },
                },
                Err(msg) => Acc::Error { message: msg },
            },
            Assertion::Enum { column, allowed } => {
                if allowed.is_empty() {
                    return Acc::Error {
                        message: format!(
                            "scan-path enum on column {column:?}: allowed set is empty \
                             (would reject every non-NULL row)"
                        ),
                    };
                }
                match column_index(schema, column) {
                    Ok(col_idx) => match schema.field(col_idx).data_type() {
                        DataType::Utf8 => Acc::Enum {
                            column: column.clone(),
                            col_idx,
                            allowed: allowed.iter().cloned().collect(),
                            allowed_len: allowed.len(),
                            miss_count: 0,
                        },
                        other => Acc::Error {
                            message: format!(
                                "scan-path enum on column {column:?}: unsupported Arrow type \
                                 {other:?} (supported: Utf8)"
                            ),
                        },
                    },
                    Err(msg) => Acc::Error { message: msg },
                }
            }
            Assertion::RowCount { low, high } => {
                if low.is_none() && high.is_none() {
                    return Acc::Error {
                        message: "scan-path row_count: at least one of low / high must be set \
                                  (asserts nothing otherwise)"
                            .into(),
                    };
                }
                Acc::RowCount {
                    low: *low,
                    high: *high,
                    count: 0,
                }
            }
            Assertion::PercentileBetween {
                column,
                p,
                low,
                high,
            } => {
                if !p.is_finite() || !(0.0..=1.0).contains(p) {
                    return Acc::Error {
                        message: format!(
                            "scan-path percentile_between on column {column:?}: \
                             p must be in [0.0, 1.0] (got {p})"
                        ),
                    };
                }
                match column_index(schema, column) {
                    Ok(col_idx) => {
                        // Reuse `between`'s supported-numeric set.
                        if !is_supported_numeric(schema.field(col_idx).data_type()) {
                            Acc::Error {
                                message: format!(
                                    "scan-path percentile_between on column {column:?}: \
                                     unsupported Arrow type {:?} (need a numeric type)",
                                    schema.field(col_idx).data_type()
                                ),
                            }
                        } else {
                            Acc::PercentileBetween {
                                column: column.clone(),
                                col_idx,
                                p: *p,
                                low: *low,
                                high: *high,
                                values: Vec::new(),
                            }
                        }
                    }
                    Err(msg) => Acc::Error { message: msg },
                }
            }
            Assertion::Freshness {
                column,
                within_seconds,
            } => {
                if *within_seconds < 0 {
                    return Acc::Error {
                        message: format!(
                            "scan-path freshness on column {column:?}: \
                             within_seconds is negative ({within_seconds})"
                        ),
                    };
                }
                match column_index(schema, column) {
                    Ok(col_idx) => match schema.field(col_idx).data_type() {
                        DataType::Timestamp(unit, _) => Acc::Freshness {
                            column: column.clone(),
                            col_idx,
                            unit: *unit,
                            within_seconds: *within_seconds,
                            max_value: None,
                        },
                        other => Acc::Error {
                            message: format!(
                                "scan-path freshness on column {column:?}: \
                                 unsupported Arrow type {other:?} (need Timestamp)"
                            ),
                        },
                    },
                    Err(msg) => Acc::Error { message: msg },
                }
            }
            // Other variants land in later phases. Until then,
            // return Error so the run still produces a Summary
            // instead of panicking.
            #[allow(unreachable_patterns)]
            other => Acc::Error {
                message: format!("scan-path evaluator does not yet support {other:?}"),
            },
        }
    }

    fn update(&mut self, batch: &RecordBatch) {
        match self {
            Acc::NotNull {
                col_idx,
                null_count,
                ..
            } => {
                let arr = batch.column(*col_idx);
                *null_count += arr.null_count() as u64;
            }
            Acc::UniqueI64 {
                col_idx,
                seen,
                dup_count,
                ..
            } => {
                let arr = batch
                    .column(*col_idx)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("UniqueI64 requires Int64 column (validated at build)");
                for i in 0..arr.len() {
                    if arr.is_null(i) {
                        // NULLs are not counted as duplicates of
                        // each other (same semantic as Postgres'
                        // GROUP BY in S-2.4 — pair with NotNull
                        // to forbid them).
                        continue;
                    }
                    if !seen.insert(arr.value(i)) {
                        *dup_count += 1;
                    }
                }
            }
            Acc::UniqueStr {
                col_idx,
                seen,
                dup_count,
                ..
            } => {
                let arr = batch
                    .column(*col_idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("UniqueStr requires Utf8 column (validated at build)");
                for i in 0..arr.len() {
                    if arr.is_null(i) {
                        continue;
                    }
                    if !seen.insert(arr.value(i).to_owned()) {
                        *dup_count += 1;
                    }
                }
            }
            Acc::Between {
                col_idx,
                low,
                high,
                oob_count,
                ..
            } => {
                let arr = batch.column(*col_idx);
                match count_oob(arr.as_ref(), *low, *high) {
                    Ok(n) => *oob_count += n,
                    Err(e) => {
                        // Promote to a terminal Error acc so the
                        // finalized AssertionResult carries the
                        // diagnostic. We replace `self` in place.
                        *self = Acc::Error { message: e };
                    }
                }
            }
            Acc::Regex {
                col_idx,
                re,
                miss_count,
                ..
            } => {
                let arr = batch
                    .column(*col_idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("Regex requires Utf8 column (validated at build)");
                for i in 0..arr.len() {
                    if arr.is_null(i) {
                        continue;
                    }
                    if !re.is_match(arr.value(i)) {
                        *miss_count += 1;
                    }
                }
            }
            Acc::Enum {
                col_idx,
                allowed,
                miss_count,
                ..
            } => {
                let arr = batch
                    .column(*col_idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("Enum requires Utf8 column (validated at build)");
                for i in 0..arr.len() {
                    if arr.is_null(i) {
                        continue;
                    }
                    if !allowed.contains(arr.value(i)) {
                        *miss_count += 1;
                    }
                }
            }
            Acc::RowCount { count, .. } => {
                *count += batch.num_rows() as i64;
            }
            Acc::PercentileBetween {
                col_idx, values, ..
            } => {
                let arr = batch.column(*col_idx);
                if let Err(e) = collect_numeric_into(arr.as_ref(), values) {
                    *self = Acc::Error { message: e };
                }
            }
            Acc::Freshness {
                col_idx,
                unit,
                max_value,
                ..
            } => {
                let arr = batch.column(*col_idx);
                let batch_max = match unit {
                    TimeUnit::Second => {
                        max_ts(arr.as_any().downcast_ref::<TimestampSecondArray>().unwrap())
                    }
                    TimeUnit::Millisecond => max_ts(
                        arr.as_any()
                            .downcast_ref::<TimestampMillisecondArray>()
                            .unwrap(),
                    ),
                    TimeUnit::Microsecond => max_ts(
                        arr.as_any()
                            .downcast_ref::<TimestampMicrosecondArray>()
                            .unwrap(),
                    ),
                    TimeUnit::Nanosecond => max_ts(
                        arr.as_any()
                            .downcast_ref::<TimestampNanosecondArray>()
                            .unwrap(),
                    ),
                };
                if let Some(b) = batch_max {
                    *max_value = Some(max_value.map_or(b, |m| m.max(b)));
                }
            }
            Acc::Error { .. } => {}
        }
    }

    fn finalize(self, assertion_index: usize) -> AssertionResult {
        match self {
            Acc::NotNull {
                column, null_count, ..
            } => {
                if null_count == 0 {
                    pass(assertion_index)
                } else {
                    fail(
                        assertion_index,
                        format!("column {column:?} has {null_count} NULL row(s); expected 0"),
                    )
                }
            }
            Acc::UniqueI64 {
                column, dup_count, ..
            }
            | Acc::UniqueStr {
                column, dup_count, ..
            } => {
                if dup_count == 0 {
                    pass(assertion_index)
                } else {
                    fail(
                        assertion_index,
                        format!(
                            "column {column:?} has {dup_count} value(s) appearing more than once"
                        ),
                    )
                }
            }
            Acc::Between {
                column,
                low,
                high,
                oob_count,
                ..
            } => {
                if oob_count == 0 {
                    pass(assertion_index)
                } else {
                    fail(
                        assertion_index,
                        format!("column {column:?} has {oob_count} row(s) outside [{low}, {high}]"),
                    )
                }
            }
            Acc::Regex {
                column,
                pattern,
                miss_count,
                ..
            } => {
                if miss_count == 0 {
                    pass(assertion_index)
                } else {
                    fail(
                        assertion_index,
                        format!(
                            "column {column:?} has {miss_count} row(s) not matching pattern \
                             {pattern:?}"
                        ),
                    )
                }
            }
            Acc::Enum {
                column,
                allowed_len,
                miss_count,
                ..
            } => {
                if miss_count == 0 {
                    pass(assertion_index)
                } else {
                    fail(
                        assertion_index,
                        format!(
                            "column {column:?} has {miss_count} row(s) outside allowed set \
                             ({allowed_len} value(s))"
                        ),
                    )
                }
            }
            Acc::RowCount { low, high, count } => {
                if let Some(lo) = low {
                    if count < lo {
                        return fail(
                            assertion_index,
                            format!("table has {count} row(s); expected at least {lo}"),
                        );
                    }
                }
                if let Some(hi) = high {
                    if count > hi {
                        return fail(
                            assertion_index,
                            format!("table has {count} row(s); expected at most {hi}"),
                        );
                    }
                }
                pass(assertion_index)
            }
            Acc::PercentileBetween {
                column,
                p,
                low,
                high,
                mut values,
                ..
            } => {
                if values.is_empty() {
                    return AssertionResult {
                        assertion_index,
                        verdict: Verdict::Error,
                        message: Some(format!(
                            "column {column:?}: no non-NULL values; \
                             cannot evaluate percentile"
                        )),
                    };
                }
                // Nearest-rank: idx = floor(p * (n - 1)).
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let n = values.len();
                let idx = (p * (n as f64 - 1.0)).floor() as usize;
                let v = values[idx];
                if v >= low && v <= high {
                    pass(assertion_index)
                } else {
                    fail(
                        assertion_index,
                        format!(
                            "column {column:?}: P{:.0} = {v} is outside [{low}, {high}]",
                            p * 100.0
                        ),
                    )
                }
            }
            Acc::Freshness {
                column,
                unit,
                within_seconds,
                max_value,
                ..
            } => match max_value {
                None => fail(
                    assertion_index,
                    format!("column {column:?}: no rows; cannot evaluate freshness"),
                ),
                Some(max) => {
                    let now_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let max_secs = ts_to_seconds(max, unit);
                    let age = now_secs - max_secs;
                    if age <= within_seconds {
                        pass(assertion_index)
                    } else {
                        fail(
                            assertion_index,
                            format!(
                                "column {column:?}: most recent value is {age}s old; \
                                 expected within {within_seconds}s"
                            ),
                        )
                    }
                }
            },
            Acc::Error { message } => AssertionResult {
                assertion_index,
                verdict: Verdict::Error,
                message: Some(message),
            },
        }
    }
}

/// Maximum non-NULL value in a typed timestamp array, or `None`
/// if the array is empty/all-NULL.
fn max_ts<T>(arr: &arrow::array::PrimitiveArray<T>) -> Option<i64>
where
    T: arrow::datatypes::ArrowPrimitiveType<Native = i64>,
{
    let mut m: Option<i64> = None;
    for i in 0..arr.len() {
        if arr.is_null(i) {
            continue;
        }
        let v = arr.value(i);
        m = Some(m.map_or(v, |cur| cur.max(v)));
    }
    m
}

fn ts_to_seconds(value: i64, unit: TimeUnit) -> i64 {
    match unit {
        TimeUnit::Second => value,
        TimeUnit::Millisecond => value / 1_000,
        TimeUnit::Microsecond => value / 1_000_000,
        TimeUnit::Nanosecond => value / 1_000_000_000,
    }
}

fn pass(assertion_index: usize) -> AssertionResult {
    AssertionResult {
        assertion_index,
        verdict: Verdict::Pass,
        message: None,
    }
}

fn fail(assertion_index: usize, msg: String) -> AssertionResult {
    AssertionResult {
        assertion_index,
        verdict: Verdict::Fail,
        message: Some(msg),
    }
}

fn column_index(schema: &Schema, column: &str) -> Result<usize, String> {
    schema.index_of(column).map_err(|_| {
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        format!("column {column:?} not found in scanner schema (available: {names:?})")
    })
}

/// True for the numeric Arrow types `between` + `percentile_between`
/// know how to handle. Kept in lockstep with the `count_oob` and
/// `collect_numeric_into` match arms below.
fn is_supported_numeric(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64
    )
}

/// Append every non-NULL value in `arr` (cast to f64) into `out`.
/// Returns Err with a diagnostic if `arr`'s type is not in the
/// supported numeric set — caller promotes to `Acc::Error`.
fn collect_numeric_into(arr: &dyn Array, out: &mut Vec<f64>) -> Result<(), String> {
    macro_rules! collect_typed {
        ($ty:ty) => {{
            let typed = arr
                .as_any()
                .downcast_ref::<$ty>()
                .expect("downcast matched data_type");
            for i in 0..typed.len() {
                if typed.is_null(i) {
                    continue;
                }
                out.push(typed.value(i) as f64);
            }
        }};
    }
    match arr.data_type() {
        DataType::Int8 => collect_typed!(Int8Array),
        DataType::Int16 => collect_typed!(Int16Array),
        DataType::Int32 => collect_typed!(Int32Array),
        DataType::Int64 => collect_typed!(Int64Array),
        DataType::UInt8 => collect_typed!(UInt8Array),
        DataType::UInt16 => collect_typed!(UInt16Array),
        DataType::UInt32 => collect_typed!(UInt32Array),
        DataType::UInt64 => collect_typed!(UInt64Array),
        DataType::Float32 => collect_typed!(Float32Array),
        DataType::Float64 => collect_typed!(Float64Array),
        other => {
            return Err(format!(
                "scan-path numeric collection: unsupported Arrow type {other:?}"
            ));
        }
    }
    Ok(())
}

/// Count out-of-range values in a numeric Arrow array. Skips NULLs.
/// Supported types match the typical numeric output of DuckDB +
/// Parquet scans; Decimal is a TBD until distribution assertions
/// in Phase 3 force the issue.
fn count_oob(arr: &dyn Array, low: f64, high: f64) -> Result<u64, String> {
    macro_rules! count_typed {
        ($ty:ty) => {{
            let typed = arr
                .as_any()
                .downcast_ref::<$ty>()
                .expect("downcast matched data_type");
            let mut n: u64 = 0;
            for i in 0..typed.len() {
                if typed.is_null(i) {
                    continue;
                }
                let v = typed.value(i) as f64;
                if v < low || v > high {
                    n += 1;
                }
            }
            n
        }};
    }
    let n = match arr.data_type() {
        DataType::Int8 => count_typed!(Int8Array),
        DataType::Int16 => count_typed!(Int16Array),
        DataType::Int32 => count_typed!(Int32Array),
        DataType::Int64 => count_typed!(Int64Array),
        DataType::UInt8 => count_typed!(UInt8Array),
        DataType::UInt16 => count_typed!(UInt16Array),
        DataType::UInt32 => count_typed!(UInt32Array),
        DataType::UInt64 => count_typed!(UInt64Array),
        DataType::Float32 => count_typed!(Float32Array),
        DataType::Float64 => count_typed!(Float64Array),
        other => {
            return Err(format!(
                "scan-path between: unsupported Arrow type {other:?} \
                 (supported: signed/unsigned integers + Float32/64)"
            ));
        }
    };
    Ok(n)
}
