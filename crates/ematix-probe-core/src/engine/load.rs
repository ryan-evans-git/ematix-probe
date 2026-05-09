//! Engine-side load-probe types: assertion DSL + plan + summary.
//!
//! Mirrors `engine::data` but for load tests: a probe describes
//! how to drive a target (constant-rate scheduler in v0.1) and
//! what statistical assertions to apply to the resulting samples.
//!
//! Reuses `Verdict`, `AssertionResult`, `RunSummary`, and
//! `reduce_verdict` from `engine::data` so callers see one
//! consistent verdict-reduction story across data + load probes.

pub mod postgres;
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
    /// Actual achieved request rate (samples / wall-clock
    /// seconds) must be at or above `threshold_rps`. Counts
    /// every attempted request including connection failures —
    /// this is the "did the scheduler keep up?" assertion.
    /// Pair with `ErrorRateBelow` if you also want to assert
    /// the requests succeeded.
    ThroughputAbove { threshold_rps: f64 },
    /// Every sample's `status_code` must be in `allowed`.
    /// Connection failures (no `status_code`) count as
    /// violations. Empty `allowed` is rejected at evaluation
    /// (asserts nothing — every sample would violate). Use
    /// alongside or instead of `ErrorRateBelow` when you want
    /// strict status-code conformance rather than a tolerated
    /// failure rate.
    StatusCodeIn { allowed: Vec<u16> },
}

/// Scheduling discipline. v0.1 ships `ConstantRate` (open
/// model — fires Ticks at a target RPS regardless of how slow
/// the target is); `VirtualUsers` (closed model — N concurrent
/// workers each looping request → wait → request) lands in
/// S-8.2.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum LoadMode {
    /// Open-model load. Target throughput in req/s. Real
    /// throughput on a busy runner may drift below this; the
    /// scheduler from S-6.4 documents acceptable drift.
    ConstantRate { rps: f64 },
    /// Closed-model load. `count` concurrent virtual users
    /// each loop "request → wait → request" until the plan
    /// duration elapses. Achieved RPS depends on per-request
    /// latency.
    VirtualUsers { count: usize },
}

/// A complete load-probe execution plan.
#[derive(Debug, Clone)]
pub struct LoadPlan {
    pub target: HttpTarget,
    pub duration: Duration,
    /// Scheduling discipline. See [`LoadMode`].
    pub mode: LoadMode,
    /// Warmup window. Samples whose `tick_index` falls in
    /// `[0, floor(warmup_secs * rps))` are dropped before
    /// evaluation — first-connection DNS / TLS / connection
    /// setup skews early latencies. `Duration::ZERO` (the
    /// default) means no warmup. Must be strictly less than
    /// `duration` or `evaluate_load` returns `Verdict::Error`
    /// for every assertion.
    pub warmup: Duration,
    pub assertions: Vec<LoadAssertion>,
}

impl LoadPlan {
    /// Convenience for callers that want the per-second tick
    /// budget without bringing the [`LoadProfile`] trait into
    /// scope. See `LoadProfile::nominal_rps`.
    pub fn nominal_rps(&self) -> Option<f64> {
        <Self as LoadProfile>::nominal_rps(self)
    }
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

/// Target-agnostic view a load plan must expose for the evaluator.
/// Both [`LoadPlan`] (HTTP) and [`postgres::PgLoadPlan`] implement
/// it so [`evaluate_load`] is one entry point regardless of target
/// type. The evaluator only consumes timing + assertions, never
/// the target details.
pub trait LoadProfile {
    fn duration(&self) -> Duration;
    fn warmup(&self) -> Duration;
    fn mode(&self) -> LoadMode;
    fn assertions(&self) -> &[LoadAssertion];

    /// Convenience for evaluators that need the per-second tick
    /// budget. Returns the configured rps for `ConstantRate`;
    /// for non-RPS-driven modes returns `None`.
    fn nominal_rps(&self) -> Option<f64> {
        match self.mode() {
            LoadMode::ConstantRate { rps } => Some(rps),
            LoadMode::VirtualUsers { .. } => None,
        }
    }
}

impl LoadProfile for LoadPlan {
    fn duration(&self) -> Duration {
        self.duration
    }
    fn warmup(&self) -> Duration {
        self.warmup
    }
    fn mode(&self) -> LoadMode {
        self.mode
    }
    fn assertions(&self) -> &[LoadAssertion] {
        &self.assertions
    }
}

/// Evaluate a load plan against a buffered slice of `Sample`s.
/// Generic over the plan type via [`LoadProfile`] so HTTP and
/// Postgres plans share one entry point. Mirrors
/// `engine::scan::evaluate` for data probes — produces a
/// `RunSummary` with one `AssertionResult` per `LoadAssertion`.
pub fn evaluate_load<P: LoadProfile>(plan: &P, samples: &[Sample]) -> RunSummary {
    // Warmup must leave a non-empty measurable window. Surfaces
    // as Error per assertion so callers see a clear diagnostic
    // rather than e.g. a divide-by-zero panic in throughput.
    let warmup = plan.warmup();
    let duration = plan.duration();
    if warmup >= duration {
        let results: Vec<AssertionResult> = plan
            .assertions()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                acc_error(
                    i,
                    format!(
                        "warmup ({warmup:?}) must be < duration ({duration:?}) — no measurable window"
                    ),
                )
            })
            .collect();
        return RunSummary {
            verdict: reduce_verdict(&results),
            assertions: results,
        };
    }

    // Filter out samples in the warmup window. For ConstantRate,
    // tick i fires at i / rps seconds, so "in warmup" iff
    // `i / rps < warmup_secs`. Closed-model (VirtualUsers) has no
    // fixed rps, so all samples count — closed-model warmup would
    // need a time-based filter (deferred).
    let warmup_ticks = match plan.nominal_rps() {
        Some(rps) => (warmup.as_secs_f64() * rps).floor() as u64,
        None => 0,
    };
    let measured: Vec<Sample> = samples
        .iter()
        .filter(|s| s.tick_index >= warmup_ticks)
        .cloned()
        .collect();

    let effective = duration.saturating_sub(warmup);
    let results: Vec<AssertionResult> = plan
        .assertions()
        .iter()
        .enumerate()
        .map(|(i, a)| eval_one(i, a, effective, &measured))
        .collect();
    RunSummary {
        verdict: reduce_verdict(&results),
        assertions: results,
    }
}

fn eval_one(
    idx: usize,
    assertion: &LoadAssertion,
    effective_duration: Duration,
    samples: &[Sample],
) -> AssertionResult {
    match assertion {
        LoadAssertion::P99Under {
            metric,
            threshold_ms,
        } => eval_p99_under(idx, metric, *threshold_ms, samples),
        LoadAssertion::ErrorRateBelow { threshold } => {
            eval_error_rate_below(idx, *threshold, samples)
        }
        LoadAssertion::ThroughputAbove { threshold_rps } => {
            // Effective measurement window passed in by
            // evaluate_load, already > 0 (warmup guard above).
            eval_throughput_above(idx, *threshold_rps, effective_duration, samples)
        }
        LoadAssertion::StatusCodeIn { allowed } => eval_status_code_in(idx, allowed, samples),
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

fn eval_error_rate_below(idx: usize, threshold: f64, samples: &[Sample]) -> AssertionResult {
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return acc_error(
            idx,
            format!("ErrorRateBelow: threshold must be in [0.0, 1.0] (got {threshold})"),
        );
    }
    if samples.is_empty() {
        return acc_error(
            idx,
            "ErrorRateBelow: no samples to compute error rate on".to_string(),
        );
    }
    // "Error" = connection failure OR non-2xx HTTP response.
    // Both are user-visible outage signal; conflating them into
    // one rate matches k6 / vegeta / locust.
    let total = samples.len();
    let bad = samples
        .iter()
        .filter(|s| {
            s.error.is_some() || !matches!(s.status_code, Some(c) if (200..300).contains(&c))
        })
        .count();
    let rate = bad as f64 / total as f64;
    if rate < threshold {
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
                "error_rate = {rate:.4} ({bad} of {total} samples); expected < {threshold:.4}"
            )),
        }
    }
}

fn eval_throughput_above(
    idx: usize,
    threshold_rps: f64,
    duration: Duration,
    samples: &[Sample],
) -> AssertionResult {
    if !threshold_rps.is_finite() || threshold_rps < 0.0 {
        return acc_error(
            idx,
            format!("ThroughputAbove: threshold_rps must be >= 0 (got {threshold_rps})"),
        );
    }
    if samples.is_empty() {
        return acc_error(
            idx,
            "ThroughputAbove: no samples to compute rate on".to_string(),
        );
    }
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        return acc_error(
            idx,
            "ThroughputAbove: plan duration is zero (no wall-clock budget)".to_string(),
        );
    }
    let actual_rps = samples.len() as f64 / secs;
    if actual_rps >= threshold_rps {
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
                "throughput = {actual_rps:.2} rps ({} samples / {secs:.2}s); \
                 expected >= {threshold_rps:.2} rps",
                samples.len()
            )),
        }
    }
}

fn eval_status_code_in(idx: usize, allowed: &[u16], samples: &[Sample]) -> AssertionResult {
    if allowed.is_empty() {
        return acc_error(
            idx,
            "StatusCodeIn: allowed set is empty (would violate every sample)".to_string(),
        );
    }
    if samples.is_empty() {
        return acc_error(idx, "StatusCodeIn: no samples to check".to_string());
    }
    // Pre-collect into a HashSet for O(1) membership; allowed
    // is typically small (1-5 codes) so the constant matters
    // less than readability.
    let allowed_set: std::collections::HashSet<u16> = allowed.iter().copied().collect();
    let mut violations: Vec<String> = Vec::new();
    for s in samples {
        match s.status_code {
            Some(c) if allowed_set.contains(&c) => {}
            Some(c) => violations.push(format!("tick #{}: status {c}", s.tick_index)),
            None => violations.push(format!(
                "tick #{}: connection error ({})",
                s.tick_index,
                s.error.as_deref().unwrap_or("unknown")
            )),
        }
    }
    if violations.is_empty() {
        AssertionResult {
            assertion_index: idx,
            verdict: Verdict::Pass,
            message: None,
        }
    } else {
        // Truncate the violation list so a 1M-sample run
        // doesn't produce a 1M-line message.
        let shown = violations
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let extra = if violations.len() > 5 {
            format!(" (+{} more)", violations.len() - 5)
        } else {
            String::new()
        };
        AssertionResult {
            assertion_index: idx,
            verdict: Verdict::Fail,
            message: Some(format!(
                "{} sample(s) with status_code outside allowed set {:?}: {}{}",
                violations.len(),
                allowed,
                shown,
                extra
            )),
        }
    }
}

fn acc_error(idx: usize, msg: String) -> AssertionResult {
    AssertionResult {
        assertion_index: idx,
        verdict: Verdict::Error,
        message: Some(msg),
    }
}
