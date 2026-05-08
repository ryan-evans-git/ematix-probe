//! Constant-rate scheduler for load probes.
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

use std::time::{Duration, Instant};

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
