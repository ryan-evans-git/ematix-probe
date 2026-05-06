//! Postgres data adapter.
//!
//! S-2.2 lands the connection plumbing: parse a URL, build a
//! `deadpool-postgres` pool, validate with `SELECT 1`. Per-assertion
//! pushdown SQL (S-2.3 `not_null`, S-2.4 `unique`, S-2.5 `between`)
//! is added in subsequent stories.

use std::str::FromStr;

use async_trait::async_trait;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::{Config, NoTls};

use crate::adapters::data::{AdapterError, DataAdapter};
use crate::engine::data::{AssertionResult, ProbePlan, RunSummary, Verdict};

/// Pooled Postgres adapter.
///
/// Connection is validated eagerly in `connect`: a `SELECT 1` round-
/// trip means a successful return value guarantees credentials,
/// network reachability, and database existence are all good.
/// Subsequent `execute` calls reuse the pool.
pub struct PostgresAdapter {
    pool: Pool,
}

impl PostgresAdapter {
    /// Open a pooled connection to the given Postgres URL and
    /// validate it. URL syntax matches `tokio_postgres::Config`
    /// (libpq-style).
    pub async fn connect(url: &str) -> Result<Self, AdapterError> {
        let pg_config = Config::from_str(url)
            .map_err(|e| AdapterError::Config(format!("invalid postgres URL: {e}")))?;
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
        let pool = Pool::builder(mgr)
            .build()
            .map_err(|e| AdapterError::Connection(format!("pool builder failed: {e}")))?;

        // Eager validation: round-trip SELECT 1. We pin the type to
        // BIGINT so the row.get is unambiguous regardless of server
        // default integer width.
        let client = pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one("SELECT 1::int8", &[])
            .await
            .map_err(|e| AdapterError::Query(format!("validation SELECT 1 failed: {e}")))?;
        let val: i64 = row.get(0);
        if val != 1 {
            return Err(AdapterError::Query(format!(
                "validation SELECT 1 returned {val}, expected 1"
            )));
        }

        Ok(Self { pool })
    }

    /// Pool accessor for the per-assertion handlers added in
    /// S-2.3..S-2.5.
    #[allow(dead_code)]
    pub(crate) fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait]
impl DataAdapter for PostgresAdapter {
    async fn execute(&self, plan: &ProbePlan) -> Result<RunSummary, AdapterError> {
        let results: Vec<AssertionResult> = Vec::with_capacity(plan.assertions.len());
        // S-2.3..S-2.5 dispatch on `Assertion` variants here. Until
        // those variants exist, `plan.assertions` is provably empty
        // (the enum has no variants), so the result vector is too.
        let verdict = reduce_verdict(&results);
        Ok(RunSummary {
            verdict,
            assertions: results,
        })
    }
}

/// Combine per-assertion verdicts into the run-level verdict.
/// Public-crate-visible so all data adapters can share it.
pub(crate) fn reduce_verdict(results: &[AssertionResult]) -> Verdict {
    if results.iter().any(|r| r.verdict == Verdict::Error) {
        Verdict::Error
    } else if results.iter().any(|r| r.verdict == Verdict::Fail) {
        Verdict::Fail
    } else {
        Verdict::Pass
    }
}
