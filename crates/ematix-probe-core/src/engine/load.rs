//! Engine-side load-probe types: assertion DSL + plan + summary.
//!
//! Mirrors `engine::data` but for load tests: a probe describes
//! how to drive a target (constant-rate scheduler in v0.1) and
//! what statistical assertions to apply to the resulting samples.
//!
//! Reuses `Verdict`, `AssertionResult`, `RunSummary`, and
//! `reduce_verdict` from `engine::data` so callers see one
//! consistent verdict-reduction story across data + load probes.

pub mod scheduler;

use std::time::Duration;

/// HTTP target for a load probe. v0.1 only supports `GET`.
/// Headers / body / auth are out of scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTarget {
    pub method: String,
    pub url: String,
}

impl HttpTarget {
    /// `HttpTarget::get(url)` — convenience for the v0.1
    /// GET-only path.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET".into(),
            url: url.into(),
        }
    }
}

/// Statistical assertions applied to the samples collected during
/// a load run. Each variant declares one threshold the engine
/// will check at finalize.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LoadAssertion {
    /// The 99th-percentile of `metric` (in milliseconds for
    /// `"latency_ms"`) must be at most `threshold_ms`.
    /// v0.1 only computes `metric == "latency_ms"`; other
    /// strings are reserved for future extension.
    P99Under { metric: String, threshold_ms: f64 },
    /// The fraction of non-2xx responses must be below
    /// `threshold` (0.0..=1.0). 0.01 = 1%.
    ErrorRateBelow { threshold: f64 },
}

/// A complete load-probe execution plan.
#[derive(Debug, Clone)]
pub struct LoadPlan {
    pub target: HttpTarget,
    pub duration: Duration,
    /// Constant target throughput in requests per second. Real
    /// throughput on a slow runner / network may drift below
    /// this; the scheduler in S-6.4 documents acceptable drift.
    pub rps: f64,
    pub assertions: Vec<LoadAssertion>,
}
