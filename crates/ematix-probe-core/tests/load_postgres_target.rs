//! S-8.4 — `PostgresTarget` + `LoadQuery` shape.
//!
//! Just the *types*: a load plan that drives Postgres needs to
//! describe (a) how to connect (DSN) and (b) what parameterized
//! query to fire. Per PRD risk: parameter values must travel
//! separately from the SQL string — there is no way to interpolate
//! values into the SQL via this API. The S-8.5 adapter consumes
//! these and binds via `tokio-postgres` prepared statements.

use ematix_probe_core::engine::load::postgres::{LoadQuery, PostgresTarget, QueryParam};

#[test]
fn load_query_round_trips_sql_and_params() {
    let q = LoadQuery::new("SELECT * FROM users WHERE id = $1 AND active = $2")
        .param(QueryParam::Int(42))
        .param(QueryParam::Bool(true));
    assert_eq!(q.sql(), "SELECT * FROM users WHERE id = $1 AND active = $2");
    assert_eq!(
        q.params(),
        &[QueryParam::Int(42), QueryParam::Bool(true)]
    );
}

#[test]
fn load_query_with_no_params_is_valid() {
    let q = LoadQuery::new("SELECT now()");
    assert_eq!(q.sql(), "SELECT now()");
    assert!(q.params().is_empty());
}

#[test]
fn query_param_variants_cover_basic_pg_types() {
    // Just prove the variants exist and compare by value.
    assert_eq!(QueryParam::Int(1), QueryParam::Int(1));
    assert_eq!(QueryParam::Text("hi".into()), QueryParam::Text("hi".into()));
    assert_eq!(QueryParam::Bool(false), QueryParam::Bool(false));
    assert_eq!(QueryParam::Float(1.5), QueryParam::Float(1.5));
    assert_eq!(QueryParam::Null, QueryParam::Null);
    assert_ne!(QueryParam::Int(1), QueryParam::Int(2));
}

#[test]
fn postgres_target_holds_dsn_and_query() {
    let target = PostgresTarget::new(
        "postgres://app:secret@localhost:5432/app",
        LoadQuery::new("SELECT 1"),
    );
    assert_eq!(target.dsn, "postgres://app:secret@localhost:5432/app");
    assert_eq!(target.query.sql(), "SELECT 1");
}

#[test]
fn load_query_param_ordering_is_preserved() {
    // $1 / $2 / $3 binding order matters — params must come back
    // in the order added.
    let q = LoadQuery::new("INSERT INTO t(a,b,c) VALUES ($1,$2,$3)")
        .param(QueryParam::Int(1))
        .param(QueryParam::Text("two".into()))
        .param(QueryParam::Float(3.0));
    assert_eq!(
        q.params(),
        &[
            QueryParam::Int(1),
            QueryParam::Text("two".into()),
            QueryParam::Float(3.0),
        ]
    );
}
