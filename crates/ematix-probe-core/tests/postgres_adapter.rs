//! S-2.2 — `PostgresAdapter` integration tests against a real
//! Postgres instance via `testcontainers`.
//!
//! These tests require a running Docker daemon. CI runners on
//! `ubuntu-latest` provide one; locally, install Docker Desktop or
//! colima. Tests are tagged with `#[cfg_attr(...)]` so a missing
//! Docker daemon yields a clean skip rather than a failure (TBD —
//! for v0.1 we just let it fail loudly on missing Docker so the
//! issue surfaces immediately).

use ematix_probe_core::adapters::data::postgres::PostgresAdapter;
use ematix_probe_core::{DataAdapter, ProbePlan, Verdict};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn postgres_adapter_connects_and_validates() {
    let pg = Postgres::default()
        .start()
        .await
        .expect("postgres container failed to start");
    let host = pg.get_host().await.expect("host");
    let port = pg
        .get_host_port_ipv4(5432)
        .await
        .expect("mapped 5432 port");

    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let adapter = PostgresAdapter::connect(&url)
        .await
        .expect("adapter constructor must validate the connection (SELECT 1)");

    // Empty-plan path through the adapter — assertion variants land
    // in S-2.3..S-2.5; until then, an empty plan is the only flow
    // the adapter exercises end-to-end.
    let plan = ProbePlan {
        schema: None,
        table: "any_table".to_string(),
        assertions: vec![],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
    assert!(summary.assertions.is_empty());
}

#[tokio::test]
async fn postgres_adapter_connect_with_invalid_url_errors() {
    let result =
        PostgresAdapter::connect("postgres://nobody:wrong@localhost:1/nope_definitely_no_db_here")
            .await;
    assert!(
        result.is_err(),
        "expected Err for unreachable host, got Ok"
    );
}
