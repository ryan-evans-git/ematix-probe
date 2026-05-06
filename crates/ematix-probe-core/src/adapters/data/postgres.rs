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
use crate::engine::data::{Assertion, AssertionResult, ProbePlan, RunSummary, Verdict};

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
        let mut results: Vec<AssertionResult> = Vec::with_capacity(plan.assertions.len());
        for (idx, assertion) in plan.assertions.iter().enumerate() {
            let result = match assertion {
                Assertion::NotNull { column } => self.run_not_null(plan, idx, column).await?,
                Assertion::Unique { column } => self.run_unique(plan, idx, column).await?,
                Assertion::Between { column, low, high } => {
                    self.run_between(plan, idx, column, *low, *high).await?
                }
            };
            results.push(result);
        }
        Ok(RunSummary {
            verdict: reduce_verdict(&results),
            assertions: results,
        })
    }
}

impl PostgresAdapter {
    /// Pushdown SQL for `NotNull`:
    ///   `SELECT count(*) FROM <qualified-table> WHERE <col> IS NULL`
    /// Pass when count = 0; Fail with row-count detail otherwise.
    async fn run_not_null(
        &self,
        plan: &ProbePlan,
        idx: usize,
        column: &str,
    ) -> Result<AssertionResult, AdapterError> {
        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let col = quote_ident(column);
        let sql = format!("SELECT count(*) FROM {table} WHERE {col} IS NULL");

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &[])
            .await
            .map_err(|e| AdapterError::Query(format!("not_null query failed: {e}")))?;
        let null_count: i64 = row.get(0);

        if null_count == 0 {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Pass,
                message: None,
            })
        } else {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Fail,
                message: Some(format!(
                    "column {column:?} has {null_count} NULL row(s); expected 0"
                )),
            })
        }
    }

    /// Pushdown SQL for `Unique`. Counts the number of distinct
    /// values that appear more than once:
    ///   `SELECT count(*) FROM (SELECT col, count(*) c FROM <t>
    ///    GROUP BY col HAVING count(*) > 1) d`
    /// Pass when 0; Fail with "N values" detail otherwise. Postgres
    /// `GROUP BY` treats NULLs as equal, so multiple NULLs ARE
    /// reported as a duplicate — pair with `NotNull` if that's
    /// undesired.
    async fn run_unique(
        &self,
        plan: &ProbePlan,
        idx: usize,
        column: &str,
    ) -> Result<AssertionResult, AdapterError> {
        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let col = quote_ident(column);
        let sql = format!(
            "SELECT count(*) FROM \
             (SELECT {col} FROM {table} GROUP BY {col} HAVING count(*) > 1) d"
        );

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &[])
            .await
            .map_err(|e| AdapterError::Query(format!("unique query failed: {e}")))?;
        let dup_value_count: i64 = row.get(0);

        if dup_value_count == 0 {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Pass,
                message: None,
            })
        } else {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Fail,
                message: Some(format!(
                    "column {column:?} has {dup_value_count} value(s) appearing more than once"
                )),
            })
        }
    }

    /// Pushdown SQL for `Between` (inclusive range, NULL-safe):
    ///   `SELECT count(*) FROM <t> WHERE <col> < $1 OR <col> > $2`
    /// Pass at 0; Fail with out-of-range row count otherwise.
    /// `$1` / `$2` bind as `FLOAT8` so non-float columns (INT,
    /// NUMERIC, …) are implicitly cast by Postgres at compare-time.
    async fn run_between(
        &self,
        plan: &ProbePlan,
        idx: usize,
        column: &str,
        low: f64,
        high: f64,
    ) -> Result<AssertionResult, AdapterError> {
        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let col = quote_ident(column);
        // Explicit `::float8` casts on the placeholders — without
        // them Postgres infers $1 / $2 from the LHS column type
        // (e.g. INT) and tokio-postgres can't serialize the f64
        // arguments. The casts also let us probe NUMERIC, INT,
        // BIGINT, and DOUBLE PRECISION columns from one shape.
        let sql =
            format!("SELECT count(*) FROM {table} WHERE {col} < $1::float8 OR {col} > $2::float8");

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &[&low, &high])
            .await
            .map_err(|e| AdapterError::Query(format!("between query failed: {e}")))?;
        let oor_count: i64 = row.get(0);

        if oor_count == 0 {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Pass,
                message: None,
            })
        } else {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Fail,
                message: Some(format!(
                    "column {column:?} has {oor_count} row(s) outside [{low}, {high}]"
                )),
            })
        }
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

/// Quote a Postgres identifier (table or column).
/// Embedded `"` characters are doubled per the SQL standard.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Build a qualified `"schema"."table"` (or just `"table"` when no
/// schema). Both halves go through `quote_ident` so reserved words
/// and embedded quotes are safe.
fn qualified_table(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_wraps_simple_name() {
        assert_eq!(quote_ident("foo"), "\"foo\"");
    }

    #[test]
    fn quote_ident_doubles_embedded_quote() {
        assert_eq!(quote_ident("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn qualified_table_with_schema() {
        assert_eq!(
            qualified_table(Some("analytics"), "dim_customers"),
            "\"analytics\".\"dim_customers\""
        );
    }

    #[test]
    fn qualified_table_without_schema() {
        assert_eq!(qualified_table(None, "users"), "\"users\"");
    }
}
