//! Adapter-side data-probe types: trait + adapter error.
//!
//! Concrete adapters (`postgres`, `duckdb`, `parquet`) live in
//! submodules and land per sprint per docs/PI_PLAN.md.

use crate::engine::data::{ProbePlan, RunSummary};
use async_trait::async_trait;

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
/// aggregate `RunSummary`. The minimal trait surface (one method)
/// lets adapters choose pushdown vs. scan internally.
#[async_trait]
pub trait DataAdapter: Send + Sync {
    async fn execute(&self, plan: &ProbePlan) -> Result<RunSummary, AdapterError>;
}
