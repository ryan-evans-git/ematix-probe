//! S-7.1 — `LoadAssertion::ThroughputAbove` evaluator.
//!
//! Computes actual req/s as `samples.len() / wall_clock_seconds`
//! where `wall_clock_seconds` is `plan.duration` minus `warmup`
//! (S-7.3 will land warmup; for S-7.1 it's just `plan.duration`).
//! Asserts the actual rate is at or above `threshold_rps`.
//!
//! Why duration rather than (last_tick - first_tick)? The
//! scheduler emits ticks across the configured window;
//! short-running probes that hit the duration boundary should
//! score against the intended budget, not the convex hull of
//! sample times. v0.1 keeps it simple.

use std::time::Duration;

use ematix_probe_core::engine::load::{evaluate_load, HttpTarget, LoadAssertion, LoadPlan, Sample};
use ematix_probe_core::Verdict;

fn plan(duration: Duration, rps: f64, assertion: LoadAssertion) -> LoadPlan {
    LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration,
        rps,
        assertions: vec![assertion],
    }
}

fn ok_sample(idx: u64) -> Sample {
    Sample {
        tick_index: idx,
        latency: Duration::from_millis(10),
        status_code: Some(200),
        error: None,
    }
}

#[test]
fn passes_when_actual_rate_at_or_above_threshold() {
    // 10 samples over 1s → 10 req/s. Threshold 8 → pass.
    let samples: Vec<Sample> = (0..10).map(ok_sample).collect();
    let p = plan(
        Duration::from_secs(1),
        10.0,
        LoadAssertion::ThroughputAbove { threshold_rps: 8.0 },
    );
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn fails_when_actual_rate_below_threshold() {
    // 10 samples over 1s → 10 req/s. Threshold 50 → fail.
    let samples: Vec<Sample> = (0..10).map(ok_sample).collect();
    let p = plan(
        Duration::from_secs(1),
        50.0,
        LoadAssertion::ThroughputAbove { threshold_rps: 50.0 },
    );
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("rps") || msg.contains("throughput"),
        "msg should reference throughput / rps: {msg:?}"
    );
}

#[test]
fn errored_samples_still_count_toward_throughput() {
    // 50 OK + 50 errors over 1s → 100 req/s. ThroughputAbove
    // measures the *attempted* rate, not the success rate
    // (ErrorRateBelow handles the success angle).
    let mut samples: Vec<Sample> = (0..50).map(ok_sample).collect();
    for i in 50..100 {
        samples.push(Sample {
            tick_index: i,
            latency: Duration::from_secs(1),
            status_code: None,
            error: Some("conn refused".into()),
        });
    }
    let p = plan(
        Duration::from_secs(1),
        100.0,
        LoadAssertion::ThroughputAbove { threshold_rps: 90.0 },
    );
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn empty_samples_yields_error() {
    let p = plan(
        Duration::from_secs(1),
        10.0,
        LoadAssertion::ThroughputAbove { threshold_rps: 5.0 },
    );
    let summary = evaluate_load(&p, &[]);
    assert_eq!(summary.verdict, Verdict::Error);
}

#[test]
fn zero_duration_yields_error() {
    // Can't compute a rate with zero wall-clock budget.
    let p = plan(
        Duration::ZERO,
        10.0,
        LoadAssertion::ThroughputAbove { threshold_rps: 5.0 },
    );
    let summary = evaluate_load(&p, &[ok_sample(0)]);
    assert_eq!(summary.verdict, Verdict::Error);
}

#[test]
fn negative_threshold_yields_error() {
    let p = plan(
        Duration::from_secs(1),
        10.0,
        LoadAssertion::ThroughputAbove {
            threshold_rps: -1.0,
        },
    );
    let summary = evaluate_load(&p, &[ok_sample(0)]);
    assert_eq!(summary.verdict, Verdict::Error);
}
