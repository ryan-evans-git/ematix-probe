//! Postgres load-probe target shape.
//!
//! `PostgresTarget` is the SQL analogue of `HttpTarget`: where to
//! connect (`dsn`) and what work to drive (`query`). `LoadQuery`
//! splits the SQL string from its bind values so the S-8.5 adapter
//! can use prepared statements — there is no API path that
//! interpolates `QueryParam`s into the SQL.
//!
//! `PgLoadPlan` is the SQL analogue of `LoadPlan`: same `mode` /
//! `duration` / `warmup` / `assertions`, but typed against
//! `PostgresTarget`. Evaluator unification (sharing one
//! `evaluate_load` across HTTP and Postgres samples) lands in S-8.6.

use std::time::Duration;

use crate::engine::load::{LoadAssertion, LoadMode};

/// One Postgres bind value. Variants cover the common
/// `tokio-postgres` `ToSql` types we need for v0.1; richer types
/// (json, arrays, decimals) land later if examples force the issue.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryParam {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    Null,
}

/// SQL string + its ordered bind values.
///
/// Built additively: `LoadQuery::new(sql).param(...).param(...)`.
/// Position in the builder corresponds to `$1`, `$2`, ... in the
/// SQL.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadQuery {
    sql: String,
    params: Vec<QueryParam>,
}

impl LoadQuery {
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            params: Vec::new(),
        }
    }

    pub fn param(mut self, p: QueryParam) -> Self {
        self.params.push(p);
        self
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn params(&self) -> &[QueryParam] {
        &self.params
    }
}

/// A Postgres load target: DSN + the parameterized query each
/// virtual user / scheduler tick will fire.
#[derive(Debug, Clone, PartialEq)]
pub struct PostgresTarget {
    pub dsn: String,
    pub query: LoadQuery,
}

impl PostgresTarget {
    pub fn new(dsn: impl Into<String>, query: LoadQuery) -> Self {
        Self {
            dsn: dsn.into(),
            query,
        }
    }
}

/// Postgres load-probe execution plan. SQL counterpart to
/// [`crate::engine::load::LoadPlan`].
#[derive(Debug, Clone)]
pub struct PgLoadPlan {
    pub target: PostgresTarget,
    pub duration: Duration,
    pub mode: LoadMode,
    /// See `LoadPlan::warmup` — same semantics here.
    pub warmup: Duration,
    pub assertions: Vec<LoadAssertion>,
}
