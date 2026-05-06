// ematix-probe-core
//
// Engine + adapter trait + assertion DSL grow phase by phase per
// docs/PI_PLAN.md.

use async_trait::async_trait;

pub const VERSION: &str = "0.1.0-dev";

pub fn version() -> &'static str {
    VERSION
}

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
/// `NotNull` / `Unique` / `Between` in S-2.3..S-2.5;
/// `Regex` / `Enum` / `RowCount` / `Freshness` in Phase 1b;
/// distribution checks in Phase 3.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Assertion {
    // Populated in S-2.3..S-2.5.
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

/// Errors an adapter can raise. Connection / query failures bubble
/// up here; adapters do *not* return `Verdict::Error` for
/// operational failures — the engine maps `Err(AdapterError)` to a
/// run-level `Verdict::Error` so callers can distinguish a failed
/// assertion from a failed probe.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("connection failed: {0}")]
    Connection(String),
    #[error("query failed: {0}")]
    Query(String),
    #[error("invalid configuration: {0}")]
    Config(String),
}

/// Contract every data-source adapter (Postgres, DuckDB, Parquet)
/// must satisfy. `execute` runs the full plan and returns an
/// aggregate `RunSummary`. The default-trait pattern (no per-
/// assertion-type methods) keeps the trait surface small while
/// still letting adapters choose pushdown vs. scan internally.
#[async_trait]
pub trait DataAdapter: Send + Sync {
    async fn execute(&self, plan: &ProbePlan) -> Result<RunSummary, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[test]
    fn version_returns_dev_string() {
        assert_eq!(version(), "0.1.0-dev");
    }

    #[test]
    fn version_constant_matches_function() {
        assert_eq!(VERSION, version());
    }

    // S-2.1 — foundational types + DataAdapter trait. RED expects
    // `ProbePlan`, `Verdict`, `RunSummary`, `AdapterError`, and
    // `DataAdapter` to exist in scope. Fails to compile until S-2.1
    // GREEN lands them.
    #[tokio::test]
    async fn empty_plan_evaluates_pass() {
        struct StubAdapter;

        #[async_trait]
        impl DataAdapter for StubAdapter {
            async fn execute(&self, _plan: &ProbePlan) -> Result<RunSummary, AdapterError> {
                Ok(RunSummary {
                    verdict: Verdict::Pass,
                    assertions: vec![],
                })
            }
        }

        let plan = ProbePlan {
            schema: None,
            table: "any_table".to_string(),
            assertions: vec![],
        };
        let summary = StubAdapter.execute(&plan).await.unwrap();
        assert_eq!(summary.verdict, Verdict::Pass);
        assert!(summary.assertions.is_empty());
    }
}
