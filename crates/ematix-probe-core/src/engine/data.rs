//! Engine-side data-probe types: assertion DSL + plan + run summary.
//!
//! Separation of concerns vs. `crate::adapters::data`:
//!  - `engine::data` defines *what* a probe asserts (declarative).
//!  - `adapters::data` defines *how* a backend executes that
//!    declaration (Postgres pushdown SQL, DuckDB scan, …).

/// Outcome of a probe run or a single assertion.
///
/// Reduction rule (engine-side, not adapter-side):
///   any `Error` → overall `Error`,
///   else any `Fail` → overall `Fail`,
///   else `Pass`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Error,
}

/// Declarative description of one column- or table-level check the
/// engine should perform. Variants land per sprint:
/// `NotNull` (S-2.3); `Unique` (S-2.4); `Between` (S-2.5);
/// `Regex` / `Enum` / `RowCount` / `Freshness` in Phase 1b;
/// distribution checks in Phase 3.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Assertion {
    /// `column IS NULL` count must be zero.
    NotNull { column: String },
    /// All non-NULL values in `column` must appear at most once.
    /// (Postgres treats NULLs as distinct in GROUP BY, so they
    /// don't violate uniqueness — pair with `NotNull` to forbid
    /// them.)
    Unique { column: String },
    /// All non-NULL values in `column` must lie within the
    /// inclusive range `[low, high]`. NULL values are *not*
    /// counted as violations (SQL `NULL < x` is unknown); pair
    /// with `NotNull` to forbid them too.
    Between { column: String, low: f64, high: f64 },
    /// All non-NULL values in `column` must match the POSIX regex
    /// `pattern` (Postgres `~` operator). NULL values are *not*
    /// counted as violations; pair with `NotNull` to forbid them.
    /// `pattern` is the POSIX regex string as accepted by Postgres;
    /// it is *not* a Python `re` pattern, though most common
    /// character-class syntax overlaps.
    Regex { column: String, pattern: String },
    /// All non-NULL values in `column` must appear in `allowed`.
    /// NULL values are *not* counted as violations (`NULL NOT IN
    /// (...)` is NULL, which `WHERE` treats as false); pair with
    /// `NotNull` to forbid them. Empty `allowed` is rejected at
    /// adapter time (every non-NULL row would violate, which is
    /// almost certainly user error).
    Enum {
        column: String,
        allowed: Vec<String>,
    },
    /// Table-level: `count(*)` must lie within `[low, high]`,
    /// where either bound may be `None` to denote "unbounded on
    /// that side". `low: Some(n)` is "at least n"; `high: Some(n)`
    /// is "at most n". Both `None` is rejected at adapter time
    /// (asserts nothing).
    RowCount { low: Option<i64>, high: Option<i64> },
    /// Table-level: the most recent value of `column` must be no
    /// older than `within_seconds`. Adapter computes
    /// `now() - MAX(<column>)` and fails when the gap exceeds the
    /// threshold. Empty tables fail (no data → no freshness
    /// signal). Negative `within_seconds` is rejected at adapter
    /// time. The Python side parses duration strings like
    /// `"24h"` / `"6h"` / `"30m"` into seconds.
    Freshness { column: String, within_seconds: i64 },
    /// The `p`-th percentile of the (non-NULL) values in `column`
    /// must lie in `[low, high]` inclusive. `p ∈ [0.0, 1.0]`.
    /// Scan-path only in v0.1 — pushdown adapters (Postgres) return
    /// `Verdict::Error` with a "scan-path only" message until a
    /// future story justifies a `percentile_cont` translation.
    /// Empty / all-NULL columns produce `Error` ("not enough data").
    PercentileBetween {
        column: String,
        p: f64,
        low: f64,
        high: f64,
    },
    /// Distinct-value count in `column` (NULLs excluded) must lie
    /// in `[low, high]`, where either bound may be `None` for
    /// "unbounded on that side". Same `Option<i64>` shape as
    /// `RowCount`. Both `None` is rejected at adapter time.
    /// Scan-path only in v0.1 — Postgres pushdown via `count(distinct
    /// col)` is straightforward but waits on a real ask.
    CardinalityBetween {
        column: String,
        low: Option<i64>,
        high: Option<i64>,
    },
    /// Strict equality check on the source schema: same field
    /// names, same Arrow `DataType`s, same order. Nullability
    /// deliberately not checked in v0.1 (DuckDB/Parquet readers
    /// often surface columns as nullable even when source data
    /// has no NULLs). Empty `fields` list rejected at adapter
    /// time. Scan-path only in v0.1 — pushing this down to
    /// Postgres would require an information_schema query, which
    /// is doable but waits on a real ask.
    SchemaMatch {
        fields: Vec<(String, arrow::datatypes::DataType)>,
    },
}

/// A complete probe execution plan: which table to probe + the
/// assertions to evaluate against it.
#[derive(Debug, Clone)]
pub struct ProbePlan {
    pub schema: Option<String>,
    pub table: String,
    pub assertions: Vec<Assertion>,
}

/// Per-assertion outcome. Index points back into
/// `ProbePlan.assertions`; `message` carries adapter-specific detail
/// shown in failure reports.
#[derive(Debug, Clone)]
pub struct AssertionResult {
    pub assertion_index: usize,
    pub verdict: Verdict,
    pub message: Option<String>,
}

/// Aggregate result of executing a `ProbePlan` against an adapter.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub verdict: Verdict,
    pub assertions: Vec<AssertionResult>,
}

/// Reduce per-assertion verdicts into the run-level verdict.
/// Any `Error` → overall `Error`; else any `Fail` → overall `Fail`;
/// else `Pass`. Used by both pushdown adapters and the scan-path
/// evaluator so they reach the same conclusion from the same inputs.
pub fn reduce_verdict(results: &[AssertionResult]) -> Verdict {
    if results.iter().any(|r| r.verdict == Verdict::Error) {
        Verdict::Error
    } else if results.iter().any(|r| r.verdict == Verdict::Fail) {
        Verdict::Fail
    } else {
        Verdict::Pass
    }
}
