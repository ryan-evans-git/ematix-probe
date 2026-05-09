//! S-7.2 — `LoadAssertion::StatusCodeIn` evaluator.
//!
//! Every sample's `status_code` must be in the `allowed` set.
//! Connection-level failures (`status_code: None`) count as
//! violations — same treatment as `ErrorRateBelow`. Empty
//! `allowed` is rejected at evaluation (asserts nothing, every
//! sample would violate).

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

fn s(idx: u64, status: Option<u16>, error: Option<&str>) -> Sample {
    Sample {
        tick_index: idx,
        latency: Duration::from_millis(10),
        status_code: status,
        error: error.map(String::from),
    }
}

#[test]
fn passes_when_every_status_in_allowed() {
    let samples = vec![
        s(0, Some(200), None),
        s(1, Some(204), None),
        s(2, Some(200), None),
    ];
    let p = plan(LoadAssertion::StatusCodeIn {
        allowed: vec![200, 204],
    });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Pass);
}

#[test]
fn fails_when_any_status_outside_allowed() {
    let samples = vec![
        s(0, Some(200), None),
        s(1, Some(500), None),
        s(2, Some(200), None),
    ];
    let p = plan(LoadAssertion::StatusCodeIn {
        allowed: vec![200, 204],
    });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.contains("500"),
        "msg should mention the offending status: {msg:?}"
    );
}

#[test]
fn connection_errors_count_as_violations() {
    let samples = vec![
        s(0, Some(200), None),
        s(1, None, Some("connection refused")),
    ];
    let p = plan(LoadAssertion::StatusCodeIn { allowed: vec![200] });
    let summary = evaluate_load(&p, &samples);
    assert_eq!(summary.verdict, Verdict::Fail);
}

#[test]
fn empty_allowed_yields_error() {
    let p = plan(LoadAssertion::StatusCodeIn { allowed: vec![] });
    let summary = evaluate_load(&p, &[s(0, Some(200), None)]);
    assert_eq!(summary.verdict, Verdict::Error);
    let msg = summary.assertions[0].message.as_ref().expect("message");
    assert!(
        msg.to_lowercase().contains("empty") || msg.to_lowercase().contains("allowed"),
        "msg should mention empty/allowed: {msg:?}"
    );
}

#[test]
fn empty_samples_yields_error() {
    let p = plan(LoadAssertion::StatusCodeIn { allowed: vec![200] });
    let summary = evaluate_load(&p, &[]);
    assert_eq!(summary.verdict, Verdict::Error);
}
