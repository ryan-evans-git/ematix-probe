//! S-4.5 — `DuckDbAdapter`: scan-path adapter backed by an
//! in-process DuckDB instance.
//!
//! Tests use an in-memory DuckDB so they don't need any setup
//! beyond the `duckdb` crate compiling. The adapter should
//! produce the same RunSummary shape as the Postgres adapter for
//! identical seed data — same Verdict/message contract, same
//! reduce_verdict reduction.

use ematix_probe_core::adapters::data::duckdb::DuckDbAdapter;
use ematix_probe_core::{Assertion, DataAdapter, ProbePlan, Verdict};

fn plan(assertions: Vec<Assertion>) -> ProbePlan {
    ProbePlan {
        schema: None,
        table: "users".into(),
        assertions,
    }
}

async fn fresh_adapter_with(seed_sql: &str) -> DuckDbAdapter {
    let adapter = DuckDbAdapter::open(":memory:").expect("open in-memory duckdb");
    adapter.execute_setup(seed_sql).await.expect("seed SQL");
    adapter
}

#[tokio::test]
async fn duckdb_runs_passing_probe_via_scan_path() {
    let a = fresh_adapter_with(
        "CREATE TABLE users (
            id BIGINT,
            email VARCHAR,
            age DOUBLE
        );
         INSERT INTO users VALUES
            (1, 'a@x.com', 25.0),
            (2, 'b@y.org', 40.0),
            (3, 'c@z.io',  33.0);",
    )
    .await;

    let p = plan(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique { column: "id".into() },
        Assertion::Between {
            column: "age".into(),
            low: 0.0,
            high: 120.0,
        },
    ]);
    let summary = a.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass, "unexpected: {summary:?}");
    assert_eq!(summary.assertions.len(), 3);
}

#[tokio::test]
async fn duckdb_surfaces_failures_per_assertion() {
    let a = fresh_adapter_with(
        "CREATE TABLE users (id BIGINT, email VARCHAR, age DOUBLE);
         INSERT INTO users VALUES
            (1, 'a@x.com',     25.0),
            (2, NULL,          40.0),
            (1, 'c@z.io',     200.0);",
    )
    .await;

    let p = plan(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique { column: "id".into() },
        Assertion::Between {
            column: "age".into(),
            low: 0.0,
            high: 120.0,
        },
    ]);
    let summary = a.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    assert!(
        summary
            .assertions
            .iter()
            .all(|r| r.verdict == Verdict::Fail),
        "expected all 3 assertions to fail; got {summary:?}"
    );
}

#[tokio::test]
async fn duckdb_table_level_assertions_work() {
    // row_count + a Utf8 enum on the same scan, exercising the
    // table-level + column-level dispatch under one plan.
    let a = fresh_adapter_with(
        "CREATE TABLE users (id BIGINT, country VARCHAR);
         INSERT INTO users VALUES
            (1, 'US'), (2, 'CA'), (3, 'US');",
    )
    .await;

    let p = plan(vec![
        Assertion::RowCount {
            low: Some(1),
            high: Some(100),
        },
        Assertion::Enum {
            column: "country".into(),
            allowed: vec!["US".into(), "CA".into(), "MX".into()],
        },
    ]);
    let summary = a.execute(&p).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn duckdb_missing_table_yields_error() {
    let a = DuckDbAdapter::open(":memory:").expect("open");
    let p = plan(vec![Assertion::NotNull {
        column: "id".into(),
    }]);
    let result = a.execute(&p).await;
    assert!(result.is_err(), "missing table should be a connect/query error, got: {result:?}");
}
