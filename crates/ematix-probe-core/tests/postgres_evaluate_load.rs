//! S-8.6 — `evaluate_load` accepts a `PgLoadPlan` and produces
//! the same `RunSummary` shape as for an `HttpTarget`-rooted plan.
//!
//! Sample is the shared currency, so the four assertion variants
//! (P99Under / ErrorRateBelow / ThroughputAbove / StatusCodeIn) all
//! work against postgres samples without modification. This test
//! exercises each variant through the postgres-typed plan to keep
//! the contract honest.

use std::time::Duration;

use ematix_probe_core::engine::data::Verdict;
use ematix_probe_core::engine::load::postgres::{LoadQuery, PgLoadPlan, PostgresTarget};
use ematix_probe_core::engine::load::{evaluate_load, LoadAssertion, LoadMode, Sample};

fn pg_plan(assertions: Vec<LoadAssertion>) -> PgLoadPlan {
    PgLoadPlan {
        target: PostgresTarget::new("postgres://x/y", LoadQuery::new("SELECT 1")),
        duration: Duration::from_secs(1),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::ZERO,
        assertions,
    }
}

fn ok_sample(tick: u64, latency_ms: u64) -> Sample {
    Sample {
        tick_index: tick,
        latency: Duration::from_millis(latency_ms),
        status_code: Some(200),
        error: None,
    }
}

fn err_sample(tick: u64) -> Sample {
    Sample {
        tick_index: tick,
        latency: Duration::from_millis(1),
        status_code: None,
        error: Some("acquire: ...".into()),
    }
}

#[test]
fn evaluate_load_accepts_pg_plan_and_runs_p99_under() {
    let plan = pg_plan(vec![LoadAssertion::P99Under {
        metric: "latency_ms".into(),
        threshold_ms: 50.0,
    }]);
    let samples: Vec<Sample> = (0..100).map(|i| ok_sample(i, 10)).collect();
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn evaluate_load_pg_plan_error_rate_below() {
    let plan = pg_plan(vec![LoadAssertion::ErrorRateBelow { threshold: 0.05 }]);
    let mut samples: Vec<Sample> = (0..100).map(|i| ok_sample(i, 5)).collect();
    samples[0] = err_sample(0); // 1/100 = 0.01 < 0.05
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn evaluate_load_pg_plan_error_rate_below_fails() {
    let plan = pg_plan(vec![LoadAssertion::ErrorRateBelow { threshold: 0.01 }]);
    let mut samples: Vec<Sample> = (0..100).map(|i| ok_sample(i, 5)).collect();
    for s in samples.iter_mut().take(10) {
        *s = err_sample(s.tick_index); // 10/100 = 0.10 >= 0.01
    }
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[test]
fn evaluate_load_pg_plan_throughput_above() {
    // 50 samples / 1s = 50 rps; >= 10 → Pass.
    let plan = pg_plan(vec![LoadAssertion::ThroughputAbove {
        threshold_rps: 10.0,
    }]);
    let samples: Vec<Sample> = (0..50).map(|i| ok_sample(i, 5)).collect();
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn evaluate_load_pg_plan_status_code_in() {
    // PostgresLoadAdapter maps success → Some(200). StatusCodeIn
    // with allowed=[200] should pass on a clean run and fail when
    // any sample carries an error (None status_code).
    let plan = pg_plan(vec![LoadAssertion::StatusCodeIn { allowed: vec![200] }]);
    let pass_samples: Vec<Sample> = (0..10).map(|i| ok_sample(i, 5)).collect();
    assert_eq!(evaluate_load(&plan, &pass_samples).verdict, Verdict::Pass);

    let mut fail_samples = pass_samples.clone();
    fail_samples[3] = err_sample(3);
    assert_eq!(evaluate_load(&plan, &fail_samples).verdict, Verdict::Fail);
}
