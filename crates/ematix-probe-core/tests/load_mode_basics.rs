//! S-8.1 — `LoadMode` enum + `LoadPlan.mode` field.
//!
//! v0.1 had `LoadPlan.rps: f64` directly. Phase 5 moves the
//! scheduling discipline into a sum type so VirtualUsers can join
//! ConstantRate without bolting another optional field on the
//! plan. The enum is `non_exhaustive` so future modes (ramping,
//! step, spike) don't break callers.

use std::time::Duration;

use ematix_probe_core::engine::load::{HttpTarget, LoadAssertion, LoadMode, LoadPlan};

#[test]
fn load_plan_carries_mode_instead_of_top_level_rps() {
    let plan = LoadPlan {
        target: HttpTarget::get("http://x.test"),
        duration: Duration::from_secs(5),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::ZERO,
        assertions: vec![LoadAssertion::ErrorRateBelow { threshold: 0.01 }],
    };
    match plan.mode {
        LoadMode::ConstantRate { rps } => assert_eq!(rps, 10.0),
        _ => panic!("wrong mode variant"),
    }
}

#[test]
fn constant_rate_holds_rps() {
    let m = LoadMode::ConstantRate { rps: 25.0 };
    if let LoadMode::ConstantRate { rps } = m {
        assert_eq!(rps, 25.0);
    } else {
        panic!("expected ConstantRate");
    }
}
