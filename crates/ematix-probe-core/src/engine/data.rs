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
