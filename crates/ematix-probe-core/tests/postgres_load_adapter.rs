//! S-8.5 — `PostgresLoadAdapter` against a real Postgres
//! testcontainer.
//!
//! Mirrors `http_load_adapter*` for the SQL load path: drive a
//! parameterized query under both `ConstantRate` (open) and
//! `VirtualUsers` (closed) modes; produce one `Sample` per tick.
//! Successful queries map to `status_code: Some(200)`; SQL errors
//! land as `status_code: None, error: Some(...)`.

use std::time::Duration;

use ematix_probe_core::adapters::load::postgres::PostgresLoadAdapter;
use ematix_probe_core::engine::load::postgres::{LoadQuery, PgLoadPlan, PostgresTarget, QueryParam};
use ematix_probe_core::engine::load::LoadMode;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

async fn dsn() -> (String, testcontainers_modules::testcontainers::ContainerAsync<Postgres>) {
    let pg = Postgres::default()
        .start()
        .await
        .expect("postgres container");
    let host = pg.get_host().await.expect("host");
    let port = pg.get_host_port_ipv4(5432).await.expect("port");
    (
        format!("postgres://postgres:postgres@{host}:{port}/postgres"),
        pg,
    )
}

#[tokio::test]
async fn constant_rate_collects_successful_samples() {
    let (dsn, _pg) = dsn().await;
    let plan = PgLoadPlan {
        target: PostgresTarget::new(
            &dsn,
            LoadQuery::new("SELECT $1::int + 1").param(QueryParam::Int(41)),
        ),
        duration: Duration::from_millis(500),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = PostgresLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");
    assert!(!samples.is_empty(), "expected samples; got 0");
    for s in &samples {
        assert!(s.error.is_none(), "unexpected error: {:?}", s.error);
        assert_eq!(s.status_code, Some(200));
    }
    let mut indices: Vec<u64> = samples.iter().map(|s| s.tick_index).collect();
    indices.sort();
    let n_before = indices.len();
    indices.dedup();
    assert_eq!(indices.len(), n_before, "tick_index collisions");
}

#[tokio::test]
async fn vu_mode_collects_samples_via_vu_pool() {
    let (dsn, _pg) = dsn().await;
    let plan = PgLoadPlan {
        target: PostgresTarget::new(&dsn, LoadQuery::new("SELECT 1")),
        duration: Duration::from_millis(500),
        mode: LoadMode::VirtualUsers { count: 4 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = PostgresLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");
    assert!(
        samples.len() >= 4,
        "expected >= 4 samples (one per worker); got {}",
        samples.len()
    );
    for s in &samples {
        assert_eq!(s.status_code, Some(200));
    }
}

#[tokio::test]
async fn sql_error_lands_as_sample_error() {
    let (dsn, _pg) = dsn().await;
    let plan = PgLoadPlan {
        target: PostgresTarget::new(
            &dsn,
            LoadQuery::new("SELECT * FROM no_such_table_xyz_42"),
        ),
        duration: Duration::from_millis(200),
        mode: LoadMode::ConstantRate { rps: 5.0 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = PostgresLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");
    assert!(!samples.is_empty(), "expected samples even on SQL error");
    for s in &samples {
        assert!(
            s.error.is_some(),
            "missing-table error should populate Sample.error"
        );
        assert_eq!(s.status_code, None);
    }
}
