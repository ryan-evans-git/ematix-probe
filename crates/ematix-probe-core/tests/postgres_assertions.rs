//! S-2.3..S-2.5 + S-3.1..S-3.4 — per-assertion behavior tests against
//! a real Postgres instance via `testcontainers`.

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

#[tokio::test]
async fn unique_passes_when_all_distinct() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE orders (id SERIAL PRIMARY KEY, customer_id INT);
             INSERT INTO orders (customer_id) VALUES (1), (2), (3), (4);",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "orders".to_string(),
        assertions: vec![Assertion::Unique {
            column: "customer_id".to_string(),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
    assert_eq!(summary.assertions[0].verdict, Verdict::Pass);
}

#[tokio::test]
async fn unique_fails_when_duplicates_present() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE orders (id SERIAL PRIMARY KEY, customer_id INT);
             INSERT INTO orders (customer_id) VALUES (1), (2), (1), (3), (2), (2);",
        )
        .await
        .expect("setup");
    // customer_id 1 appears twice, customer_id 2 appears thrice.
    // Two distinct values violate uniqueness.

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "orders".to_string(),
        assertions: vec![Assertion::Unique {
            column: "customer_id".to_string(),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(msg.contains("customer_id"), "message: {msg:?}");
    assert!(
        msg.contains('2'),
        "message should mention the count of dup values (2), got: {msg:?}"
    );
}

#[tokio::test]
async fn between_passes_when_all_in_range() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE people (id SERIAL PRIMARY KEY, age INT);
             -- include both endpoints of [0, 120] to lock in inclusive semantics
             INSERT INTO people (age) VALUES (0), (5), (35), (120);",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "people".to_string(),
        assertions: vec![Assertion::Between {
            column: "age".to_string(),
            low: 0.0,
            high: 120.0,
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn between_fails_when_values_out_of_range() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE people (id SERIAL PRIMARY KEY, age INT);
             -- two violations: -1 below low, 200 above high
             INSERT INTO people (age) VALUES (5), (-1), (10), (200), (35);",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "people".to_string(),
        assertions: vec![Assertion::Between {
            column: "age".to_string(),
            low: 0.0,
            high: 120.0,
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(msg.contains("age"), "message: {msg:?}");
    assert!(
        msg.contains('2'),
        "message should report 2 out-of-range rows, got: {msg:?}"
    );
}

#[tokio::test]
async fn regex_passes_when_all_match() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE users (id SERIAL PRIMARY KEY, email TEXT);
             INSERT INTO users (email) VALUES
               ('a@x.io'), ('b@y.com'), ('carol@example.org');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "users".to_string(),
        assertions: vec![Assertion::Regex {
            column: "email".to_string(),
            pattern: r".+@.+\..+".to_string(),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
    assert_eq!(summary.assertions.len(), 1);
    assert_eq!(summary.assertions[0].verdict, Verdict::Pass);
}

#[tokio::test]
async fn regex_fails_when_any_value_violates() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE users (id SERIAL PRIMARY KEY, email TEXT);
             -- 'not-an-email' lacks the '@'+TLD shape, NULL is not
             -- a violation (NULL ~ pat is unknown).
             INSERT INTO users (email) VALUES
               ('a@x.io'), ('not-an-email'), (NULL), ('b@y.com');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "users".to_string(),
        assertions: vec![Assertion::Regex {
            column: "email".to_string(),
            pattern: r".+@.+\..+".to_string(),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(
        summary.verdict,
        Verdict::Fail,
        "one non-matching email should fail the assertion"
    );
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(
        msg.contains("email"),
        "message should reference column, got: {msg:?}"
    );
    assert!(
        msg.contains('1'),
        "message should report 1 non-matching row, got: {msg:?}"
    );
}

#[tokio::test]
async fn enum_passes_when_all_values_in_allowed_set() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE shipments (id SERIAL PRIMARY KEY, country TEXT);
             INSERT INTO shipments (country) VALUES
               ('US'), ('CA'), ('US'), ('MX'), ('CA');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "shipments".to_string(),
        assertions: vec![Assertion::Enum {
            column: "country".to_string(),
            allowed: vec!["US".to_string(), "CA".to_string(), "MX".to_string()],
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
    assert_eq!(summary.assertions[0].verdict, Verdict::Pass);
}

#[tokio::test]
async fn enum_fails_when_value_outside_allowed_set() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE shipments (id SERIAL PRIMARY KEY, country TEXT);
             -- 'ZZ' is the violation; NULL is not counted.
             INSERT INTO shipments (country) VALUES
               ('US'), ('ZZ'), (NULL), ('CA');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "shipments".to_string(),
        assertions: vec![Assertion::Enum {
            column: "country".to_string(),
            allowed: vec!["US".to_string(), "CA".to_string(), "MX".to_string()],
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(msg.contains("country"), "message: {msg:?}");
    assert!(
        msg.contains('1'),
        "message should report 1 disallowed row, got: {msg:?}"
    );
}

#[tokio::test]
async fn row_count_at_least_fails_on_empty_table() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute("CREATE TABLE events (id SERIAL PRIMARY KEY, payload TEXT);")
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "events".to_string(),
        assertions: vec![Assertion::RowCount {
            low: Some(1),
            high: None,
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(
        summary.verdict,
        Verdict::Fail,
        "empty table should fail at_least(1)"
    );
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(
        msg.contains('0'),
        "message should mention actual count 0, got: {msg:?}"
    );
    assert!(
        msg.contains('1'),
        "message should mention low bound 1, got: {msg:?}"
    );
}

#[tokio::test]
async fn row_count_at_most_fails_when_oversized() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    // Insert 1500 rows so at_most(1000) fails by 500.
    client
        .batch_execute(
            "CREATE TABLE events (id SERIAL PRIMARY KEY, payload TEXT);
             INSERT INTO events (payload) SELECT 'x' FROM generate_series(1, 1500);",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "events".to_string(),
        assertions: vec![Assertion::RowCount {
            low: None,
            high: Some(1000),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(
        msg.contains("1500"),
        "message should mention actual count 1500, got: {msg:?}"
    );
    assert!(
        msg.contains("1000"),
        "message should mention high bound 1000, got: {msg:?}"
    );
}

#[tokio::test]
async fn row_count_passes_when_in_range() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE events (id SERIAL PRIMARY KEY, payload TEXT);
             INSERT INTO events (payload) SELECT 'x' FROM generate_series(1, 50);",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "events".to_string(),
        assertions: vec![Assertion::RowCount {
            low: Some(1),
            high: Some(100),
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_passes_when_max_recent() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE events (id SERIAL PRIMARY KEY, updated_at TIMESTAMPTZ);
             -- newest row 5 minutes ago — well within a 24h window.
             INSERT INTO events (updated_at) VALUES
               (now() - interval '6 hours'),
               (now() - interval '5 minutes'),
               (now() - interval '2 hours');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "events".to_string(),
        assertions: vec![Assertion::Freshness {
            column: "updated_at".to_string(),
            within_seconds: 24 * 3600,
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[tokio::test]
async fn freshness_fails_when_max_too_old() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute(
            "CREATE TABLE events (id SERIAL PRIMARY KEY, updated_at TIMESTAMPTZ);
             -- newest row 48 hours old → fails within(24h).
             INSERT INTO events (updated_at) VALUES
               (now() - interval '72 hours'),
               (now() - interval '48 hours');",
        )
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "events".to_string(),
        assertions: vec![Assertion::Freshness {
            column: "updated_at".to_string(),
            within_seconds: 24 * 3600,
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(
        msg.contains("updated_at"),
        "message should reference column, got: {msg:?}"
    );
}

#[tokio::test]
async fn freshness_fails_on_empty_table() {
    let (_pg, url) = postgres().await;

    let client = raw_client(&url).await;
    client
        .batch_execute("CREATE TABLE events (id SERIAL PRIMARY KEY, updated_at TIMESTAMPTZ);")
        .await
        .expect("setup");

    let adapter = PostgresAdapter::connect(&url).await.expect("adapter");
    let plan = ProbePlan {
        schema: None,
        table: "events".to_string(),
        assertions: vec![Assertion::Freshness {
            column: "updated_at".to_string(),
            within_seconds: 24 * 3600,
        }],
    };
    let summary = adapter.execute(&plan).await.expect("execute");
    assert_eq!(
        summary.verdict,
        Verdict::Fail,
        "empty table provides no freshness signal — should fail"
    );
    let msg = summary.assertions[0]
        .message
        .as_ref()
        .expect("message present on fail");
    assert!(
        msg.to_lowercase().contains("no rows") || msg.to_lowercase().contains("empty"),
        "message should mention emptiness, got: {msg:?}"
    );
}
