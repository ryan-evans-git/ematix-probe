//! HTTP load adapter — drives the constant-rate scheduler
//! against an HTTP target and collects one [`Sample`] per tick.
//!
//! v0.1 surface: `collect_samples(plan)` returns the raw
//! per-tick samples; running them through `LoadAssertion`
//! evaluators is S-6.6 / S-6.7.

use std::time::{Duration, Instant};

use crate::adapters::data::AdapterError;
use crate::engine::load::scheduler::ConstantRateScheduler;
use crate::engine::load::LoadPlan;

/// One per-tick measurement.
///
/// `error` is `Some` when the request couldn't complete (DNS
/// failure, connection refused, TLS error, etc); in that case
/// `status_code` is `None`. A 4xx / 5xx response is *not* an
/// error from the adapter's perspective — that's a successful
/// HTTP round-trip with a non-2xx status, surfaced via
/// `status_code` so [`LoadAssertion::ErrorRateBelow`] can count
/// it.
#[derive(Debug, Clone)]
pub struct Sample {
    pub tick_index: u64,
    pub latency: Duration,
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

/// HTTP load adapter using `reqwest` + the constant-rate
/// scheduler. Stateless — one client per `collect_samples` call.
pub struct HttpLoadAdapter;

impl HttpLoadAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Drive the load plan and return per-tick samples. Each
    /// tick issues one request; the request runs in a spawned
    /// task so the next tick can fire on schedule even if a
    /// prior request is still in flight.
    pub async fn collect_samples(&self, plan: &LoadPlan) -> Result<Vec<Sample>, AdapterError> {
        // One client; reqwest pools connections internally.
        let client = reqwest::Client::builder()
            // Bound the per-request budget so a hung target
            // doesn't pile up indefinitely.
            .timeout(plan.duration + Duration::from_secs(5))
            .build()
            .map_err(|e| AdapterError::Connection(format!("reqwest client: {e}")))?;

        let mut sched = ConstantRateScheduler::new(plan.rps, plan.duration);
        let mut handles = Vec::new();

        while let Some(tick) = sched.next_tick().await {
            let client = client.clone();
            let url = plan.target.url.clone();
            let h = tokio::spawn(async move {
                let started = Instant::now();
                let result = client.get(&url).send().await;
                let latency = started.elapsed();
                match result {
                    Ok(resp) => Sample {
                        tick_index: tick.index,
                        latency,
                        status_code: Some(resp.status().as_u16()),
                        error: None,
                    },
                    Err(e) => Sample {
                        tick_index: tick.index,
                        latency,
                        status_code: None,
                        error: Some(e.to_string()),
                    },
                }
            });
            handles.push(h);
        }

        let mut samples = Vec::with_capacity(handles.len());
        for h in handles {
            samples.push(
                h.await
                    .map_err(|e| AdapterError::Connection(format!("join: {e}")))?,
            );
        }
        // Sort by tick_index so the output order is deterministic
        // even though spawned requests may finish out-of-order.
        samples.sort_by_key(|s| s.tick_index);
        Ok(samples)
    }
}

impl Default for HttpLoadAdapter {
    fn default() -> Self {
        Self::new()
    }
}
