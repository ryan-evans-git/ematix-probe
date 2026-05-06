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
    Enum { column: String, allowed: Vec<String> },
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
