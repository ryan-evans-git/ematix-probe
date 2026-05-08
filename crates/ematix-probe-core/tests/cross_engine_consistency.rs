//! S-5.7 — cross-engine consistency property test.
//!
//! Same seed data in Postgres + DuckDB + Parquet, same probe
//! plan, must produce the same Verdicts. Catches drift between
//! the pushdown SQL evaluator and the scan-path evaluator
//! (e.g. NULL handling, edge-of-range comparisons, regex flavor
//! differences) before user-visible inconsistency lands.
//!
//! Two suites:
//! - `core_assertions_agree`: the 7 v0.1 assertions both engines
//!   implement (`not_null`, `unique`, `between`, `regex`, `enum`,
//!   `row_count`, `freshness`). Postgres + DuckDB + Parquet all
//!   participate; all 3 RunSummaries are compared.
//! - `scan_only_assertions_agree`: the 3 Phase-3 assertions
//!   Postgres returns `Error` for. Only DuckDB + Parquet are
//!   compared.
//!
//! Postgres comes from `testcontainers`, so this test requires
//! Docker. It's written as a single tokio test that fans out so
//! one container start covers both suites.

use std::fs::File;

use ematix_probe_core::adapters::data::duckdb::DuckDbAdapter;
use ematix_probe_core::adapters::data::parquet::ParquetAdapter;
use ematix_probe_core::adapters::data::postgres::PostgresAdapter;
use ematix_probe_core::{Assertion, AssertionResult, DataAdapter, ProbePlan, RunSummary, Verdict};
use parquet::arrow::ArrowWriter;
use tempfile::TempDir;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_postgres::NoTls;

/// Postgres flavor of the seed: TIMESTAMPTZ + DOUBLE PRECISION.
const POSTGRES_SEED: &str = "
CREATE TABLE users (
    id BIGINT,
    email TEXT,
    age DOUBLE PRECISION,
    country TEXT,
    updated_at TIMESTAMPTZ DEFAULT now()
);
INSERT INTO users (id, email, age, country) VALUES
    (1, 'alice@example.com', 30, 'US'),
    (2, 'bob@example.com',   45, 'CA'),
    (3, NULL,                28, 'US'),    -- NULL email
    (1, 'carol@example.com', 200, 'GB');   -- dup id, bad age, country not in {US,CA}
";

/// DuckDB flavor of the seed (used for both DuckDb adapter
/// + the parquet seed via COPY TO).
const DUCKDB_SEED: &str = "
CREATE TABLE users (
    id BIGINT,
    email VARCHAR,
    age DOUBLE,
    country VARCHAR,
    updated_at TIMESTAMP DEFAULT now()
);
INSERT INTO users (id, email, age, country) VALUES
    (1, 'alice@example.com', 30,  'US'),
    (2, 'bob@example.com',   45,  'CA'),
    (3, NULL,                28,  'US'),
    (1, 'carol@example.com', 200, 'GB');
";

fn pp(assertions: Vec<Assertion>) -> ProbePlan {
    ProbePlan {
        schema: None,
        table: "users".into(),
        assertions,
    }
}

/// Reduce a RunSummary to (Verdict, Vec<Verdict>) for cross-engine
/// comparison — message strings differ between adapters, but
/// verdicts must agree.
fn shape(summary: &RunSummary) -> (Verdict, Vec<Verdict>) {
    (
        summary.verdict,
        summary.assertions.iter().map(|a| a.verdict).collect(),
    )
}

fn assert_shapes_equal(label: &str, a: &RunSummary, b: &RunSummary) {
    let sa = shape(a);
    let sb = shape(b);
    assert_eq!(
        sa, sb,
        "{label}: shapes diverge\n  expected: {sa:?}\n  actual:   {sb:?}\n  expected results: {:?}\n  actual results:   {:?}",
        a.assertions
            .iter()
            .map(|r| (r.assertion_index, r.verdict, r.message.as_deref()))
            .collect::<Vec<_>>(),
        b.assertions
            .iter()
            .map(|r| (r.assertion_index, r.verdict, r.message.as_deref()))
            .collect::<Vec<_>>(),
    );
}

#[tokio::test]
async fn cross_engine_verdicts_agree() {
    // ---- Spin Postgres ----
    let pg = Postgres::default()
        .start()
        .await
        .expect("postgres container failed to start");
    let host = pg.get_host().await.unwrap();
    let port = pg.get_host_port_ipv4(5432).await.unwrap();
    let pg_url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    // Seed Postgres directly via tokio-postgres.
    let (client, conn) = tokio_postgres::connect(&pg_url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client.batch_execute(POSTGRES_SEED).await.unwrap();
    drop(client);

    let pg_adapter = PostgresAdapter::connect(&pg_url).await.unwrap();

    // ---- Spin DuckDB ----
    let dir = TempDir::new().unwrap();
    let duck_path = dir
        .path()
        .join("seed.duckdb")
        .to_string_lossy()
        .into_owned();
    let duck = DuckDbAdapter::open(&duck_path).expect("open duckdb");
    duck.execute_setup(DUCKDB_SEED).await.expect("seed duckdb");

    // ---- Spin Parquet via DuckDB COPY TO ----
    let parquet_path = dir.path().join("users.parquet");
    let parquet_seed_db = dir
        .path()
        .join("parquet_seed.duckdb")
        .to_string_lossy()
        .into_owned();
    let parquet_seed = DuckDbAdapter::open(&parquet_seed_db).expect("open parquet seed");
    parquet_seed
        .execute_setup(&format!(
            "{DUCKDB_SEED}\nCOPY users TO '{}' (FORMAT PARQUET);",
            parquet_path.display()
        ))
        .await
        .expect("seed parquet");
    let parquet = ParquetAdapter::open(&parquet_path).expect("open parquet");

    // ---- Suite 1: core assertions (postgres + duckdb + parquet must agree) ----
    let core_plan = pp(vec![
        Assertion::NotNull {
            column: "email".into(),
        },
        Assertion::Unique {
            column: "id".into(),
        },
        Assertion::Between {
            column: "age".into(),
            low: 0.0,
            high: 120.0,
        },
        Assertion::Regex {
            column: "email".into(),
            pattern: r".+@.+\..+".into(),
        },
        Assertion::Enum {
            column: "country".into(),
            allowed: vec!["US".into(), "CA".into(), "MX".into()],
        },
        Assertion::RowCount {
            low: Some(1),
            high: Some(1_000),
        },
        Assertion::Freshness {
            column: "updated_at".into(),
            within_seconds: 24 * 3600,
        },
    ]);
    let pg_core = pg_adapter.execute(&core_plan).await.expect("pg core");
    let duck_core = duck.execute(&core_plan).await.expect("duckdb core");
    let parquet_core = parquet.execute(&core_plan).await.expect("parquet core");

    assert_shapes_equal("postgres vs duckdb (core)", &pg_core, &duck_core);
    assert_shapes_equal("duckdb vs parquet (core)", &duck_core, &parquet_core);

    // Spot-check the expected verdicts so a coincidental mutual
    // miscomputation across adapters can't slip through.
    let expected: Vec<Verdict> = vec![
        Verdict::Fail, // not_null email — 1 NULL row
        Verdict::Fail, // unique id — duplicate 1
        Verdict::Fail, // between age — 200 > 120
        Verdict::Pass, // regex email — only the NULL fails to "match" but NULLs skipped
        Verdict::Fail, // enum country — GB ∉ allowed
        Verdict::Pass, // row_count — 4 rows, in [1, 1000]
        Verdict::Pass, // freshness — all updated_at = now()
    ];
    assert_eq!(
        duck_core
            .assertions
            .iter()
            .map(|r| r.verdict)
            .collect::<Vec<_>>(),
        expected,
        "spot-check verdicts diverged from expected: {duck_core:?}",
    );

    // ---- Suite 2: scan-only assertions (duckdb + parquet must agree) ----
    let scan_plan = pp(vec![
        Assertion::PercentileBetween {
            column: "age".into(),
            p: 0.5,
            low: 25.0,
            high: 50.0,
        },
        Assertion::CardinalityBetween {
            column: "country".into(),
            low: Some(2),
            high: Some(10),
        },
        Assertion::SchemaMatch {
            // DuckDB + Parquet both produce these names + types
            // for the seed table.
            fields: vec![
                ("id".into(), arrow::datatypes::DataType::Int64),
                ("email".into(), arrow::datatypes::DataType::Utf8),
                ("age".into(), arrow::datatypes::DataType::Float64),
                ("country".into(), arrow::datatypes::DataType::Utf8),
                (
                    "updated_at".into(),
                    arrow::datatypes::DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        None,
                    ),
                ),
            ],
        },
    ]);
    let duck_scan = duck.execute(&scan_plan).await.expect("duckdb scan");
    let parquet_scan = parquet.execute(&scan_plan).await.expect("parquet scan");
    assert_shapes_equal("duckdb vs parquet (scan-only)", &duck_scan, &parquet_scan);

    // Postgres returns Error for all three of these; verify that
    // contract too.
    let pg_scan = pg_adapter.execute(&scan_plan).await.expect("pg scan");
    assert_eq!(
        pg_scan
            .assertions
            .iter()
            .map(|r| r.verdict)
            .collect::<Vec<_>>(),
        vec![Verdict::Error, Verdict::Error, Verdict::Error],
        "Postgres adapter should Error on all scan-only assertions: {pg_scan:?}",
    );

    // Drop the temp dir + parquet writer leftovers cleanly.
    drop(parquet);
    drop(duck);
    drop(parquet_seed);
}

// Keep these symbols imported for the docstring claim that we
// also could do bytes-level Parquet writing — they're no-ops
// here but the intent is to leave the seam open for future
// scan-only fixtures that don't go through DuckDB.
#[allow(dead_code)]
fn _phantom_arrow_writer(_w: ArrowWriter<File>, _r: AssertionResult) {}
