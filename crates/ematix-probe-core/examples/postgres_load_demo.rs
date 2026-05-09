//! End-to-end Postgres load-probe demo.
//!
//! Spins a Postgres testcontainer, seeds a small `users` table,
//! then runs a 10-VU closed-model load against
//! `SELECT * FROM users WHERE id = $1` for 2 seconds, evaluating
//! all four `LoadAssertion` variants.
//!
//! Run from the repo root (Docker daemon must be up):
//!
//!     cargo run --example postgres_load_demo --package ematix-probe-core
//!
//! No external services required — the testcontainer binds to a
//! kernel-picked port and is torn down on exit.
//!
//! Symmetric counterpart to `load_probe_demo.rs` (HTTP).

use std::time::Duration;

use ematix_probe_core::adapters::load::postgres::PostgresLoadAdapter;
use ematix_probe_core::engine::data::Verdict;
use ematix_probe_core::engine::load::postgres::{
    LoadQuery, PgLoadPlan, PostgresTarget, QueryParam,
};
use ematix_probe_core::engine::load::{evaluate_load, LoadAssertion, LoadMode};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_postgres::NoTls;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pg = Postgres::default().start().await?;
    let host = pg.get_host().await?;
    let port = pg.get_host_port_ipv4(5432).await?;
    let dsn = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    println!("Postgres testcontainer up at {dsn}");

    seed_users(&dsn).await?;

    let plan = PgLoadPlan {
        target: PostgresTarget::new(
            &dsn,
            // QueryParam::Int binds as int8 — match it on the
            // server side with `::bigint`.
            LoadQuery::new("SELECT * FROM users WHERE id = $1::bigint").param(QueryParam::Int(1)),
        ),
        duration: Duration::from_secs(2),
        mode: LoadMode::VirtualUsers { count: 10 },
        warmup: Duration::ZERO,
        assertions: vec![
            LoadAssertion::P99Under {
                metric: "latency_ms".into(),
                threshold_ms: 50.0,
            },
            LoadAssertion::ErrorRateBelow { threshold: 0.01 },
            LoadAssertion::ThroughputAbove {
                threshold_rps: 50.0,
            },
            LoadAssertion::StatusCodeIn { allowed: vec![200] },
        ],
    };

    println!(
        "Driving 10 VUs for {:?} against `SELECT * FROM users WHERE id = $1`...",
        plan.duration
    );
    let adapter = PostgresLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await?;
    println!("Collected {} samples; evaluating.\n", samples.len());

    let summary = evaluate_load(&plan, &samples);
    println!("Verdict: {:?}", summary.verdict);
    println!("Assertions:");
    for r in &summary.assertions {
        let marker = match r.verdict {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Error => " ERR",
        };
        let kind_label = match plan.assertions.get(r.assertion_index) {
            Some(LoadAssertion::P99Under { .. }) => "p99_under",
            Some(LoadAssertion::ErrorRateBelow { .. }) => "error_rate_below",
            Some(LoadAssertion::ThroughputAbove { .. }) => "throughput_above",
            Some(LoadAssertion::StatusCodeIn { .. }) => "status_code_in",
            _ => "unknown",
        };
        let line = format!("  [{marker}] {kind_label}");
        if let Some(msg) = &r.message {
            println!("{line}  -- {msg}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}

async fn seed_users(dsn: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (client, conn) = tokio_postgres::connect(dsn, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("postgres connection error: {e}");
        }
    });
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS users (id BIGINT PRIMARY KEY, name TEXT NOT NULL); \
             INSERT INTO users(id, name) VALUES (1, 'Ada'), (2, 'Grace'), (3, 'Linus') \
             ON CONFLICT (id) DO NOTHING;",
        )
        .await?;
    Ok(())
}
