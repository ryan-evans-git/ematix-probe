//! S-6.4 — `ConstantRateScheduler`: emits `Tick`s at a target
//! RPS for a configured duration.
//!
//! Tick #i is *scheduled* at `start + i / rps`. The scheduler
//! sleeps until that instant before emitting; on a busy runner
//! actual fire time may drift slightly later. Drift is recorded
//! on the Tick so downstream components can decide whether to
//! discard out-of-budget samples.

use std::time::Duration;

use ematix_probe_core::engine::load::scheduler::{ConstantRateScheduler, Tick};

#[tokio::test]
async fn ten_rps_for_one_second_yields_ten_ticks() {
    let mut sched = ConstantRateScheduler::new(10.0, Duration::from_secs(1));
    let mut count = 0;
    while let Some(_t) = sched.next_tick().await {
        count += 1;
    }
    // Strict spec: tick #i fires at start + i/rps; ticks where
    // scheduled_at < start + duration are emitted. So 10 RPS for
    // 1s emits ticks at 0.0..0.9s = 10 ticks.
    assert_eq!(count, 10);
}

#[tokio::test]
async fn ticks_carry_monotonic_indices_starting_at_zero() {
    let mut sched = ConstantRateScheduler::new(50.0, Duration::from_millis(200));
    let mut indices: Vec<u64> = Vec::new();
    while let Some(t) = sched.next_tick().await {
        indices.push(t.index);
    }
    assert!(!indices.is_empty(), "should emit some ticks");
    assert_eq!(indices[0], 0);
    for w in indices.windows(2) {
        assert_eq!(w[1], w[0] + 1, "indices must be monotonic +1");
    }
}

#[tokio::test]
async fn zero_duration_yields_zero_ticks() {
    let mut sched = ConstantRateScheduler::new(100.0, Duration::ZERO);
    assert!(sched.next_tick().await.is_none());
}

#[tokio::test]
async fn fired_at_is_at_or_after_scheduled_at() {
    let mut sched = ConstantRateScheduler::new(20.0, Duration::from_millis(250));
    let mut sampled: Vec<Tick> = Vec::new();
    while let Some(t) = sched.next_tick().await {
        sampled.push(t);
    }
    assert!(!sampled.is_empty());
    for t in &sampled {
        // The scheduler may run late but never early.
        assert!(
            t.fired_at >= t.scheduled_at,
            "tick #{} fired before its schedule: scheduled={:?}, fired={:?}",
            t.index,
            t.scheduled_at,
            t.fired_at,
        );
    }
}

#[tokio::test]
async fn rps_emission_within_loose_tolerance() {
    // 50 RPS for 200ms → expect ~10 ticks. Allow ±2 to absorb
    // the busy-CI tax. Not a perf test; just a sanity check
    // that the rate isn't 1 tick or 1000 ticks.
    let mut sched = ConstantRateScheduler::new(50.0, Duration::from_millis(200));
    let mut count = 0;
    while let Some(_t) = sched.next_tick().await {
        count += 1;
    }
    assert!(
        (8..=12).contains(&count),
        "50 RPS / 200ms expected ~10 ticks; got {count}"
    );
}
