//! S-6.6 — `LoadAssertion::P99Under` evaluator.
//!
//! v0.1 implementation: collect non-error sample latencies into a
//! Vec<f64> milliseconds, sort, pick `values[floor(0.99 * (n-1))]`.
//! Same nearest-rank method as `PercentileBetween` for data
//! probes, same v0.1 memory trade-off (O(samples)).
//!
//! Future: t-digest or OTel ExponentialHistogram for bounded-memory
//! streaming when sample counts cross the "fits in RAM" line.

use std::time::Duration;

use ematix_probe_core::engine::load::HttpTarget;
use ematix_probe_core::engine::load::{evaluate_load, LoadAssertion, LoadPlan, Sample};
use ematix_probe_core::Verdict;

fn plan(assertions: Vec<LoadAssertion>) -> LoadPlan {
    LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(1),
        rps: 1.0,
        warmup: Duration::ZERO,
        assertions,
    }
}

fn sample(idx: u64, latency_ms: u64, status: Option<u16>) -> Sample {
    Sample {
        tick_index: idx,
        latency: Duration::from_millis(latency_ms),
        status_code: status,
        error: None,
    }
}

#[test]
fn p99_passes_when_under_threshold() {
    // Latencies: 100 samples uniformly 1..100 ms.
    // P99 = value at floor(0.99 * 99) = 98 → 99 ms.
    // Threshold 200 ms → pass.
    let samples: Vec<Sample> = (1..=100)
        .map(|i| sample(i as u64, i as u64, Some(200)))
        .collect();
    let p = plan(vec![LoadAssertion::P99Under {
        metric: "latency_ms".into(),
        threshold_ms: 200.0,
    }]);
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn p99_fails_when_over_threshold() {
    let samples: Vec<Sample> = (1..=100)
        .map(|i| sample(i as u64, i as u64, Some(200)))
        .collect();
    let p = plan(vec![LoadAssertion::P99Under {
        metric: "latency_ms".into(),
        threshold_ms: 50.0,
    }]);
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(msg.contains("p99"), "msg should reference p99: {msg:?}");
    assert!(
        msg.contains("99"),
        "msg should reference observed value: {msg:?}"
    );
}

#[test]
fn p99_excludes_error_samples_from_distribution() {
    // 99 fast samples + 1 connection-error sample. The error
    // sample's "latency" is whatever the adapter recorded
    // before the timeout fired — we don't want it skewing p99.
    let mut samples: Vec<Sample> = (1..=99).map(|i| sample(i as u64, 10, Some(200))).collect();
    samples.push(Sample {
        tick_index: 99,
        latency: Duration::from_secs(30),
        status_code: None,
        error: Some("connection refused".into()),
    });
    let p = plan(vec![LoadAssertion::P99Under {
        metric: "latency_ms".into(),
        threshold_ms: 100.0,
    }]);
    let summary = evaluate_load(&p, &samples);
    // P99 should still be ~10ms (errors excluded), so well under 100.
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn p99_zero_successful_samples_yields_error() {
    // Every sample errored — no distribution to compute on.
    let samples: Vec<Sample> = (0..5)
        .map(|i| Sample {
            tick_index: i,
            latency: Duration::from_secs(30),
            status_code: None,
            error: Some("conn refused".into()),
        })
        .collect();
    let p = plan(vec![LoadAssertion::P99Under {
        metric: "latency_ms".into(),
        threshold_ms: 100.0,
    }]);
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Error);
}

#[test]
fn p99_unknown_metric_yields_error() {
    let samples: Vec<Sample> = vec![sample(0, 10, Some(200))];
    let p = plan(vec![LoadAssertion::P99Under {
        metric: "throughput_rps".into(), // not "latency_ms"
        threshold_ms: 100.0,
    }]);
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Error);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.to_lowercase().contains("metric") || msg.contains("throughput_rps"),
        "msg should call out the unknown metric: {msg:?}"
    );
}
