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

use arrow::array::{
    Array, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array, Int8Array, StringArray,
    UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use arrow::datatypes::{DataType, Schema, SchemaRef};
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
            // Other variants are scan-path TBD (S-4.4 row_count /
            // freshness). Until then, return Error so the run still
            // produces a Summary instead of panicking.
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
            Acc::Error { message } => AssertionResult {
                assertion_index,
                verdict: Verdict::Error,
                message: Some(message),
            },
        }
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
