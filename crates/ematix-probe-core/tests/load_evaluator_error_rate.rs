//! S-6.7 — `LoadAssertion::ErrorRateBelow` evaluator.
//!
//! "Error" here = connection-level failure OR non-2xx HTTP
//! response. Both signal a user-visible outage; conflating them
//! into one "error rate" matches what production load-testing
//! tools (k6, vegeta, locust) do.

use std::time::Duration;

use ematix_probe_core::engine::load::{evaluate_load, HttpTarget, LoadAssertion, LoadPlan, Sample};
use ematix_probe_core::Verdict;

fn plan(assertion: LoadAssertion) -> LoadPlan {
    LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(1),
        rps: 1.0,
        warmup: Duration::ZERO,
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

fn http_error_sample(idx: u64, code: u16) -> Sample {
    Sample {
        tick_index: idx,
        latency: Duration::from_millis(10),
        status_code: Some(code),
        error: None,
    }
}

fn conn_error_sample(idx: u64) -> Sample {
    Sample {
        tick_index: idx,
        latency: Duration::from_secs(1),
        status_code: None,
        error: Some("connection refused".into()),
    }
}

#[test]
fn passes_when_error_rate_under_threshold() {
    // 99 OK + 1 503 = 1% error rate; threshold 5% → pass.
    let mut samples: Vec<Sample> = (0..99).map(ok_sample).collect();
    samples.push(http_error_sample(99, 503));
    let p = plan(LoadAssertion::ErrorRateBelow { threshold: 0.05 });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn fails_when_error_rate_at_or_above_threshold() {
    // 80 OK + 20 5xx = 20% error rate; threshold 10% → fail.
    let mut samples: Vec<Sample> = (0..80).map(ok_sample).collect();
    for i in 80..100 {
        samples.push(http_error_sample(i, 500));
    }
    let p = plan(LoadAssertion::ErrorRateBelow { threshold: 0.10 });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("error_rate") || msg.contains("error rate"),
        "msg should reference error rate: {msg:?}"
    );
}

#[test]
fn connection_failures_count_as_errors() {
    // 50 OK + 50 connection-refused = 50% error rate.
    let mut samples: Vec<Sample> = (0..50).map(ok_sample).collect();
    for i in 50..100 {
        samples.push(conn_error_sample(i));
    }
    let p = plan(LoadAssertion::ErrorRateBelow { threshold: 0.10 });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[test]
fn four_xx_responses_count_as_errors() {
    // 90 OK + 10 4xx = 10% error rate; threshold 0.05 → fail.
    let mut samples: Vec<Sample> = (0..90).map(ok_sample).collect();
    for i in 90..100 {
        samples.push(http_error_sample(i, 404));
    }
    let p = plan(LoadAssertion::ErrorRateBelow { threshold: 0.05 });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[test]
fn empty_samples_yields_error() {
    let p = plan(LoadAssertion::ErrorRateBelow { threshold: 0.01 });
    let summary = evaluate_load(&p, &[]);
    assert_eq!(summary.verdict, Verdict::Error);
}

#[test]
fn out_of_range_threshold_yields_error() {
    for bad in [-0.1_f64, 1.1_f64, f64::NAN] {
        let p = plan(LoadAssertion::ErrorRateBelow { threshold: bad });
        let summary = evaluate_load(&p, &[ok_sample(0)]);
        assert_eq!(summary.verdict, Verdict::Error, "threshold={bad}");
    }
}
