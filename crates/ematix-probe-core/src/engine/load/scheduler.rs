//! Schedulers for load probes — open-model
//! ([`ConstantRateScheduler`]) and closed-model ([`VuPool`]).
//!
//!
//! Tick #i is *scheduled* at `start + i / rps`. The scheduler
//! sleeps until that instant before emitting; on a busy runner
//! actual fire time may drift slightly later. Drift is recorded
//! on the Tick (`fired_at - scheduled_at`) so downstream
//! components can decide whether to discard out-of-budget
//! samples.
//!
//! v0.1 only constant-rate; ramping / step / spike profiles are
//! deferred. The contract is "drive at this RPS for this long",
//! and the HTTP adapter (S-6.5) consumes the resulting tick
//! stream to issue requests.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::engine::load::Sample;

/// One scheduling event.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    pub index: u64,
    pub scheduled_at: Instant,
    pub fired_at: Instant,
}

/// Emits ticks at a constant target RPS for a fixed duration.
/// Stateful: hold a `&mut` and call `next_tick().await` until it
/// returns `None`.
pub struct ConstantRateScheduler {
    rps: f64,
    duration: Duration,
    start: Option<Instant>,
    next_index: u64,
}

impl ConstantRateScheduler {
    pub fn new(rps: f64, duration: Duration) -> Self {
        Self {
            rps,
            duration,
            start: None,
            next_index: 0,
        }
    }

    /// Wait until the next tick fires and return it. Returns
    /// `None` once the configured duration has elapsed.
    pub async fn next_tick(&mut self) -> Option<Tick> {
        // Lazy start so the first call drives the wall-clock
        // baseline (rather than `new()` having to be timed).
        let start = *self.start.get_or_insert_with(Instant::now);

        // Tick #i is scheduled at start + i / rps. Stop if that's
        // at or past the configured end.
        let tick_period = Duration::from_secs_f64(1.0 / self.rps);
        let scheduled_at = start + tick_period.mul_f64(self.next_index as f64);
        if scheduled_at >= start + self.duration {
            return None;
        }

        let now = Instant::now();
        if scheduled_at > now {
            tokio::time::sleep(scheduled_at - now).await;
        }
        let fired_at = Instant::now();
        let tick = Tick {
            index: self.next_index,
            scheduled_at,
            fired_at,
        };
        self.next_index += 1;
        Some(tick)
    }
}

/// Closed-model load: a pool of `count` workers each loop
/// "request → wait → request" until `duration` elapses. Per-tick
/// `tick_index` is assigned by an atomic counter so workers
/// never collide. The output is sorted by `tick_index` for
/// deterministic ordering.
///
/// Distinct from [`ConstantRateScheduler`]: that one fires at
/// a fixed rate regardless of how slow the work is (open model),
/// while `VuPool`'s achieved RPS depends entirely on per-work
/// latency (closed model). Standard for stress-testing a
/// service that backs off under load.
pub struct VuPool {
    count: usize,
    duration: Duration,
}

impl VuPool {
    pub fn new(count: usize, duration: Duration) -> Self {
        Self { count, duration }
    }

    /// Drive the pool. `work(tick_index)` is called once per
    /// iteration on each worker; the returned `Sample` lands
    /// in the output. Stops accepting new work once
    /// `Instant::now() >= start + duration`.
    pub async fn run<F, Fut>(&self, work: F) -> Vec<Sample>
    where
        F: Fn(u64) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Sample> + Send + 'static,
    {
        if self.duration.is_zero() {
            return Vec::new();
        }
        let next_index = Arc::new(AtomicU64::new(0));
        let deadline = Instant::now() + self.duration;
        let mut handles = Vec::with_capacity(self.count);
        for _ in 0..self.count {
            let work = work.clone();
            let next_index = next_index.clone();
            handles.push(tokio::spawn(async move {
                let mut samples: Vec<Sample> = Vec::new();
                loop {
                    if Instant::now() >= deadline {
                        break;
                    }
                    let idx = next_index.fetch_add(1, Ordering::SeqCst);
                    let s = work(idx).await;
                    samples.push(s);
                }
                samples
            }));
        }
        let mut all: Vec<Sample> = Vec::new();
        for h in handles {
            if let Ok(mut chunk) = h.await {
                all.append(&mut chunk);
            }
        }
        all.sort_by_key(|s| s.tick_index);
        all
    }
}
