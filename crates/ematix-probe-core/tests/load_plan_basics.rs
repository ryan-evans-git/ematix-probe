//! S-6.3 — `engine::load` skeleton: `LoadPlan`,
//! `LoadAssertion`, `HttpTarget`. Reuses `Verdict`,
//! `AssertionResult`, `RunSummary`, and `reduce_verdict` from
//! `engine::data` so the pass/fail/error shape is identical
//! across data + load probes.

use std::time::Duration;

use ematix_probe_core::engine::data::{reduce_verdict, AssertionResult, Verdict};
use ematix_probe_core::engine::load::{HttpTarget, LoadAssertion, LoadPlan};

#[test]
fn load_plan_holds_target_duration_rps_and_assertions() {
    let plan = LoadPlan {
        target: HttpTarget::get("http://example.com/health"),
        duration: Duration::from_secs(5),
        rps: 10.0,
        assertions: vec![
            LoadAssertion::P99Under {
                metric: "latency_ms".into(),
                threshold_ms: 200.0,
            },
            LoadAssertion::ErrorRateBelow { threshold: 0.01 },
        ],
    };
    assert_eq!(plan.target.url, "http://example.com/health");
    assert_eq!(plan.target.method, "GET");
    assert_eq!(plan.duration, Duration::from_secs(5));
    assert_eq!(plan.rps, 10.0);
    assert_eq!(plan.assertions.len(), 2);
}

#[test]
fn http_target_get_constructor_sets_method() {
    let t = HttpTarget::get("http://x.test");
    assert_eq!(t.method, "GET");
    assert_eq!(t.url, "http://x.test");
}

#[test]
fn load_uses_engine_data_verdict_reduction() {
    // Sanity: engine::data::reduce_verdict is the canonical
    // reducer for both data + load probes.
    let mixed = vec![
        AssertionResult {
            assertion_index: 0,
            verdict: Verdict::Pass,
            message: None,
        },
        AssertionResult {
            assertion_index: 1,
            verdict: Verdict::Fail,
            message: Some("over threshold".into()),
        },
    ];
    assert_eq!(reduce_verdict(&mixed), Verdict::Fail);

    let all_pass = vec![AssertionResult {
        assertion_index: 0,
        verdict: Verdict::Pass,
        message: None,
    }];
    assert_eq!(reduce_verdict(&all_pass), Verdict::Pass);

    let with_error = vec![AssertionResult {
        assertion_index: 0,
        verdict: Verdict::Error,
        message: None,
    }];
    assert_eq!(reduce_verdict(&with_error), Verdict::Error);
}

#[test]
fn load_assertion_variants_are_constructible() {
    // Just verify both v0.1 LoadAssertion variants exist and
    // hold their expected fields.
    let p99 = LoadAssertion::P99Under {
        metric: "latency_ms".into(),
        threshold_ms: 250.0,
    };
    let err = LoadAssertion::ErrorRateBelow { threshold: 0.05 };
    match p99 {
        LoadAssertion::P99Under {
            metric,
            threshold_ms,
        } => {
            assert_eq!(metric, "latency_ms");
            assert_eq!(threshold_ms, 250.0);
        }
        _ => panic!("wrong variant"),
    }
    match err {
        LoadAssertion::ErrorRateBelow { threshold } => assert_eq!(threshold, 0.05),
        _ => panic!("wrong variant"),
    }
}
