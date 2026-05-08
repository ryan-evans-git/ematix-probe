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
use crate::engine::data::{
    reduce_verdict, Assertion, AssertionResult, ProbePlan, RunSummary, Verdict,
};

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
                Assertion::Regex { column, pattern } => {
                    self.run_regex(plan, idx, column, pattern).await?
                }
                Assertion::Enum { column, allowed } => {
                    self.run_enum(plan, idx, column, allowed).await?
                }
                Assertion::RowCount { low, high } => {
                    self.run_row_count(plan, idx, *low, *high).await?
                }
                Assertion::Freshness {
                    column,
                    within_seconds,
                } => {
                    self.run_freshness(plan, idx, column, *within_seconds)
                        .await?
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

impl PostgresAdapter {
    /// Pushdown SQL for `Regex` (Postgres POSIX `!~`):
    ///   `SELECT count(*) FROM <t> WHERE <col> !~ $1`
    /// Pass at 0; Fail with non-matching row count otherwise.
    /// NULL values are not counted: `NULL !~ pat` is NULL, which
    /// `WHERE` treats as false. Pair with `NotNull` to forbid NULLs.
    async fn run_regex(
        &self,
        plan: &ProbePlan,
        idx: usize,
        column: &str,
        pattern: &str,
    ) -> Result<AssertionResult, AdapterError> {
        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let col = quote_ident(column);
        let sql = format!("SELECT count(*) FROM {table} WHERE {col} !~ $1");

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &[&pattern])
            .await
            .map_err(|e| AdapterError::Query(format!("regex query failed: {e}")))?;
        let bad_count: i64 = row.get(0);

        if bad_count == 0 {
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
                    "column {column:?} has {bad_count} row(s) not matching pattern {pattern:?}"
                )),
            })
        }
    }
}

impl PostgresAdapter {
    /// Pushdown SQL for `Enum`:
    ///   `SELECT count(*) FROM <t> WHERE <col> NOT IN ($1, $2, ...)`
    /// Pass at 0; Fail with disallowed-row count otherwise.
    /// NULL values are not counted: `NULL NOT IN (...)` is NULL,
    /// which `WHERE` treats as false. Pair with `NotNull` to forbid
    /// NULLs. Empty `allowed` is rejected as `AdapterError::Config`
    /// — every non-NULL row would otherwise violate, which is
    /// almost certainly user error.
    async fn run_enum(
        &self,
        plan: &ProbePlan,
        idx: usize,
        column: &str,
        allowed: &[String],
    ) -> Result<AssertionResult, AdapterError> {
        if allowed.is_empty() {
            return Err(AdapterError::Config(format!(
                "enum assertion on column {column:?} has empty `allowed` set"
            )));
        }

        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let col = quote_ident(column);
        let placeholders = (1..=allowed.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT count(*) FROM {table} WHERE {col} NOT IN ({placeholders})");

        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = allowed
            .iter()
            .map(|s| s as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &params)
            .await
            .map_err(|e| AdapterError::Query(format!("enum query failed: {e}")))?;
        let bad_count: i64 = row.get(0);

        if bad_count == 0 {
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
                    "column {column:?} has {bad_count} row(s) outside allowed set ({} value(s))",
                    allowed.len()
                )),
            })
        }
    }

    /// Pushdown SQL for `RowCount` (table-level):
    ///   `SELECT count(*) FROM <t>`
    /// Then compare against the bounds in Rust. Both bounds `None`
    /// is rejected as `AdapterError::Config` (asserts nothing).
    /// `low: Some(n)` is "at least n"; `high: Some(n)` is "at most n".
    async fn run_row_count(
        &self,
        plan: &ProbePlan,
        idx: usize,
        low: Option<i64>,
        high: Option<i64>,
    ) -> Result<AssertionResult, AdapterError> {
        if low.is_none() && high.is_none() {
            return Err(AdapterError::Config(
                "row_count assertion requires at least one of `low` or `high`".to_string(),
            ));
        }

        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let sql = format!("SELECT count(*) FROM {table}");

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &[])
            .await
            .map_err(|e| AdapterError::Query(format!("row_count query failed: {e}")))?;
        let count: i64 = row.get(0);

        let too_low = low.map(|lo| count < lo).unwrap_or(false);
        let too_high = high.map(|hi| count > hi).unwrap_or(false);

        if !too_low && !too_high {
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Pass,
                message: None,
            })
        } else {
            let bound_desc = match (low, high) {
                (Some(lo), Some(hi)) => format!("[{lo}, {hi}]"),
                (Some(lo), None) => format!(">= {lo}"),
                (None, Some(hi)) => format!("<= {hi}"),
                (None, None) => unreachable!("guarded above"),
            };
            Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Fail,
                message: Some(format!(
                    "table {:?} has {count} row(s); expected {bound_desc}",
                    plan.table
                )),
            })
        }
    }

    /// Pushdown SQL for `Freshness` (table-level):
    ///   `SELECT EXTRACT(EPOCH FROM (now() - MAX(<col>))) FROM <t>`
    /// The `EXTRACT(EPOCH FROM <interval>)` returns the gap in
    /// seconds as `double precision`. NULL when the table is empty
    /// (MAX of nothing is NULL → interval is NULL → extract is NULL),
    /// which we treat as Fail-with-no-rows: an empty table provides
    /// no freshness signal, so the data-quality verdict is Fail
    /// rather than Error (Error would imply we couldn't evaluate
    /// the check, but we did — there's just nothing to be fresh).
    /// Negative `within_seconds` is rejected as
    /// `AdapterError::Config`.
    async fn run_freshness(
        &self,
        plan: &ProbePlan,
        idx: usize,
        column: &str,
        within_seconds: i64,
    ) -> Result<AssertionResult, AdapterError> {
        if within_seconds < 0 {
            return Err(AdapterError::Config(format!(
                "freshness assertion on column {column:?} has negative within_seconds ({within_seconds})"
            )));
        }

        let table = qualified_table(plan.schema.as_deref(), &plan.table);
        let col = quote_ident(column);
        // Cast to double precision: in PG 14+ EXTRACT returns
        // `numeric`, which tokio-postgres can't deserialize to f64
        // without the rust_decimal feature. PG <14 returned float8
        // already; the cast is a no-op there.
        let sql = format!(
            "SELECT EXTRACT(EPOCH FROM (now() - MAX({col})))::double precision FROM {table}"
        );

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AdapterError::Connection(format!("acquire failed: {e}")))?;
        let row = client
            .query_one(&sql, &[])
            .await
            .map_err(|e| AdapterError::Query(format!("freshness query failed: {e}")))?;
        let age_seconds: Option<f64> = row.get(0);

        match age_seconds {
            None => Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Fail,
                message: Some(format!(
                    "table {:?} has no rows; cannot evaluate freshness on column {column:?}",
                    plan.table
                )),
            }),
            Some(age) if age <= within_seconds as f64 => Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Pass,
                message: None,
            }),
            Some(age) => Ok(AssertionResult {
                assertion_index: idx,
                verdict: Verdict::Fail,
                message: Some(format!(
                    "column {column:?}: most recent value is {age:.0}s old; \
                     expected within {within_seconds}s"
                )),
            }),
        }
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
