//! S-7.3 — `LoadPlan::warmup` + sample-window filtering.
//!
//! Samples with `tick_index < floor(warmup_secs * rps)` are
//! dropped before evaluation. Rationale: the first connection's
//! DNS / TLS handshake / connection setup skews early latencies;
//! warmup gives the system time to settle.
//!
//! Throughput's denominator becomes `(duration - warmup)` so a
//! 1-RPS / 10s-duration probe with 5s warmup measures against
//! the 5 samples in the post-warmup window, not all 10.

use std::time::Duration;

use ematix_probe_core::engine::load::{
    evaluate_load, HttpTarget, LoadAssertion, LoadMode, LoadPlan, Sample,
};
use ematix_probe_core::Verdict;

fn s(idx: u64, latency_ms: u64, status: Option<u16>) -> Sample {
    Sample {
        tick_index: idx,
        latency: Duration::from_millis(latency_ms),
        status_code: status,
        error: None,
    }
}

#[test]
fn warmup_drops_early_samples_for_p99() {
    // 100 RPS for 1s → tick_index 0..99. Warmup 100ms → drop
    // ticks 0..9 (the first 10). The dropped samples are
    // intentionally slow (1000ms); the kept ones are fast (10ms).
    // Without warmup, p99 would be ~1000ms; with warmup, ~10ms.
    let mut samples = Vec::new();
    for i in 0..10 {
        samples.push(s(i, 1000, Some(200))); // slow warmup
    }
    for i in 10..100 {
        samples.push(s(i, 10, Some(200))); // fast steady state
    }

    let plan = LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(1),
        mode: LoadMode::ConstantRate { rps: 100.0 },
        warmup: Duration::from_millis(100),
        assertions: vec![LoadAssertion::P99Under {
            metric: "latency_ms".into(),
            threshold_ms: 50.0,
        }],
    };
    let summary = evaluate_load(&plan, &samples);
    // Without warmup, p99 ≈ 1000ms → fail. With warmup, ≈10ms → pass.
    assert_eq!(
        summary.verdict,
        Verdict::Pass,
        "warmup should hide the slow head: {summary:?}"
    );
}

#[test]
fn throughput_denominator_excludes_warmup() {
    // 10 RPS / 2s / 1s warmup → expected effective duration = 1s.
    // After warmup-filter: 10 samples (ticks 10..19) over 1s = 10 RPS.
    let samples: Vec<Sample> = (0..20).map(|i| s(i, 10, Some(200))).collect();

    let plan = LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(2),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::from_secs(1),
        assertions: vec![LoadAssertion::ThroughputAbove { threshold_rps: 9.5 }],
    };
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn warmup_zero_is_default_and_filters_nothing() {
    let samples: Vec<Sample> = (0..10).map(|i| s(i, 10, Some(200))).collect();
    let plan = LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(1),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::ZERO,
        assertions: vec![LoadAssertion::ThroughputAbove {
            threshold_rps: 10.0,
        }],
    };
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn warmup_at_or_above_duration_yields_error() {
    let plan = LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(1),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::from_secs(2), // > duration
        assertions: vec![LoadAssertion::ThroughputAbove { threshold_rps: 1.0 }],
    };
    let samples: Vec<Sample> = (0..10).map(|i| s(i, 10, Some(200))).collect();
    let summary = evaluate_load(&plan, &samples);
    assert_eq!(summary.verdict, Verdict::Error);
}
