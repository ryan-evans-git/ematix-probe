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

use crate::engine::data::{reduce_verdict, AssertionResult, RunSummary, Verdict};

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

/// One per-tick measurement collected by a load adapter.
///
/// `error` is `Some` only on connection-level failures (DNS, TCP,
/// TLS); 4xx/5xx responses are successful round-trips with a
/// non-2xx `status_code`. Lives in `engine::load` (not
/// `adapters::load::http`) because evaluators consume it.
#[derive(Debug, Clone)]
pub struct Sample {
    pub tick_index: u64,
    pub latency: Duration,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

/// Evaluate a `LoadPlan` against a buffered slice of `Sample`s.
/// Mirrors `engine::scan::evaluate` for data probes — produces
/// a `RunSummary` with one `AssertionResult` per `LoadAssertion`.
pub fn evaluate_load(plan: &LoadPlan, samples: &[Sample]) -> RunSummary {
    let results: Vec<AssertionResult> = plan
        .assertions
        .iter()
        .enumerate()
        .map(|(i, a)| eval_one(i, a, samples))
        .collect();
    RunSummary {
        verdict: reduce_verdict(&results),
        assertions: results,
    }
}

fn eval_one(idx: usize, assertion: &LoadAssertion, samples: &[Sample]) -> AssertionResult {
    match assertion {
        LoadAssertion::P99Under {
            metric,
            threshold_ms,
        } => eval_p99_under(idx, metric, *threshold_ms, samples),
        LoadAssertion::ErrorRateBelow { threshold } => {
            eval_error_rate_below(idx, *threshold, samples)
        }
    }
}

fn eval_p99_under(
    idx: usize,
    metric: &str,
    threshold_ms: f64,
    samples: &[Sample],
) -> AssertionResult {
    if metric != "latency_ms" {
        return acc_error(
            idx,
            format!(
                "P99Under: unknown metric {metric:?} — v0.1 only supports \
                 \"latency_ms\""
            ),
        );
    }
    let mut latencies_ms: Vec<f64> = samples
        .iter()
        .filter(|s| s.error.is_none())
        .map(|s| s.latency.as_secs_f64() * 1000.0)
        .collect();
    if latencies_ms.is_empty() {
        return acc_error(
            idx,
            "P99Under: no successful samples to compute p99 on".to_string(),
        );
    }
    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = latencies_ms.len();
    let p99_idx = (0.99_f64 * (n as f64 - 1.0)).floor() as usize;
    let p99 = latencies_ms[p99_idx];
    if p99 <= threshold_ms {
        AssertionResult {
            assertion_index: idx,
            verdict: Verdict::Pass,
            message: None,
        }
    } else {
        AssertionResult {
            assertion_index: idx,
            verdict: Verdict::Fail,
            message: Some(format!(
                "p99 latency = {p99:.1}ms; expected <= {threshold_ms:.1}ms"
            )),
        }
    }
}

fn eval_error_rate_below(idx: usize, _threshold: f64, _samples: &[Sample]) -> AssertionResult {
    // Stub — real evaluator lands in S-6.7 with its own
    // RED→GREEN cycle. Until then, surface as Error so the
    // verdict reduction still produces a well-formed Summary
    // instead of falsely passing.
    acc_error(
        idx,
        "ErrorRateBelow: scan-path evaluator not yet implemented (S-6.7)".into(),
    )
}

fn acc_error(idx: usize, msg: String) -> AssertionResult {
    AssertionResult {
        assertion_index: idx,
        verdict: Verdict::Error,
        message: Some(msg),
    }
}
