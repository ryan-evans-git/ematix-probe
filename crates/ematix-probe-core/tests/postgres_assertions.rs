//! S-2.3..S-2.5 — per-assertion behavior tests against a real
//! Postgres instance via `testcontainers`.

use async_trait::async_trait;
use ematix_probe_core::adapters::data::postgres::PostgresAdapter;
use ematix_probe_core::{Assertion, DataAdapter, ProbePlan, Verdict};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ContainerAsync;
use tokio_postgres::NoTls;

// `async_trait` is referenced only inside test-only helper impls;
// reference it once at top-level so unused-deps lints stay quiet.
#[async_trait]
trait _Marker {}

/// Spin a fresh Postgres testcontainer + return (container, url).
/// Container handle must outlive its url use — Drop kills the
/// container. Returning the handle keeps it alive for the whole
/// test scope.
async fn postgres() -> (ContainerAsync<Postgres>, String) {
    let pg = Postgres::default()
        .start()
        .await
        .expect("postgres container failed to start");
    let host = pg.get_host().await.expect("host");
    let port = pg.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    (pg, url)
}

/// Open a one-shot tokio-postgres client for setup statements.
async fn raw_client(url: &str) -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(url, NoTls)
        .await
        .expect("raw connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

#[tokio::test]
async fn not_null_passes_when_no_nulls() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE users (id SERIAL PRIMARY KEY, email TEXT NOT NULL);
             INSERT INTO users (email) VALUES ('a@x'), ('b@y'), ('c@z');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "users".to_string(),
        assertions: vec![Assertion::NotNull {
            column: "email".to_string(),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
    assert_eq!(summary.assertions.len(), 1);
    assert_eq!(summary.assertions[0].verdict, Verdict::Pass);
    assert_eq!(summary.assertions[0].assertion_index, 0);
}

#[tokio::test]
async fn not_null_fails_when_any_null() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE users (id SERIAL PRIMARY KEY, email TEXT);
             INSERT INTO users (email) VALUES ('a@x'), (NULL), (NULL), ('b@y');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "users".to_string(),
        assertions: vec![Assertion::NotNull {
            column: "email".to_string(),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(
        summary.verdict,
        Verdict::Fail,
        "two NULL emails should fail the assertion"
    );
    let r = &summary.assertions[0];
    assert_eq!(r.verdict, Verdict::Fail);
    let msg = r.message.as_ref().expect("message present on fail");
    assert!(
        msg.contains("email"),
        "message should reference the column name, got: {msg:?}"
    );
    assert!(
        msg.contains('2'),
        "message should reference the failing-row count (2), got: {msg:?}"
    );
}
