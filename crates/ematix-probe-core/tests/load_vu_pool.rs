//! S-8.2 — `VuPool` scheduler: closed-model load.
//!
//! N workers each loop "request → wait → request" until the
//! configured duration elapses. Each work call produces one
//! Sample; the pool collects them (sorted by tick_index) and
//! returns.
//!
//! Distinct from `ConstantRateScheduler` (open model — fires at
//! fixed RPS regardless of how slow work is). VuPool's achieved
//! RPS depends entirely on per-work latency.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ematix_probe_core::engine::load::scheduler::VuPool;
use ematix_probe_core::engine::load::Sample;

fn fast_work(tick_index: u64) -> Sample {
    Sample {
        tick_index,
        latency: Duration::from_millis(1),
        status_code: Some(200),
        error: None,
    }
}

#[tokio::test]
async fn vu_pool_zero_duration_yields_zero_samples() {
    let pool = VuPool::new(4, Duration::ZERO);
    let samples = pool
        .run(|i| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            fast_work(i)
        })
        .await;
    assert!(
        samples.is_empty(),
        "zero duration → zero samples; got {}",
        samples.len()
    );
}

#[tokio::test]
async fn vu_pool_runs_each_worker_at_least_once() {
    // 4 workers for 200ms, each work() = 10ms sleep. Lower bound:
    // each worker starts at most once before the deadline → 4
    // samples minimum even in the worst case.
    let pool = VuPool::new(4, Duration::from_millis(200));
    let samples = pool
        .run(|i| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            fast_work(i)
        })
        .await;
    assert!(
        samples.len() >= 4,
        "expected >= 4 samples (one per worker); got {}",
        samples.len()
    );
}

#[tokio::test]
async fn vu_pool_tick_indices_are_unique() {
    // Each work invocation should get a distinct tick_index.
    let pool = VuPool::new(8, Duration::from_millis(150));
    let samples = pool
        .run(|i| async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            fast_work(i)
        })
        .await;
    let mut indices: Vec<u64> = samples.iter().map(|s| s.tick_index).collect();
    indices.sort();
    let len_before = indices.len();
    indices.dedup();
    assert_eq!(
        indices.len(),
        len_before,
        "tick_index collisions: {indices:?} (started with {len_before})"
    );
}

#[tokio::test]
async fn vu_pool_samples_sorted_by_tick_index() {
    let pool = VuPool::new(4, Duration::from_millis(100));
    let samples = pool
        .run(|i| async move {
            // Vary the work latency so workers finish out of order.
            tokio::time::sleep(Duration::from_millis(i % 5)).await;
            fast_work(i)
        })
        .await;
    let indices: Vec<u64> = samples.iter().map(|s| s.tick_index).collect();
    let mut sorted = indices.clone();
    sorted.sort();
    assert_eq!(indices, sorted, "samples should be sorted by tick_index");
}

#[tokio::test]
async fn vu_pool_observes_concurrency() {
    // 4 workers should observe peak in-flight count ≥ 2 (any
    // value short of "all serialized" — we just want to prove
    // they're not running one-at-a-time).
    let in_flight = Arc::new(AtomicU64::new(0));
    let peak = Arc::new(AtomicU64::new(0));
    let pool = VuPool::new(4, Duration::from_millis(100));
    let in_flight2 = in_flight.clone();
    let peak2 = peak.clone();
    let samples = pool
        .run(move |i| {
            let inf = in_flight2.clone();
            let pk = peak2.clone();
            async move {
                let cur = inf.fetch_add(1, Ordering::SeqCst) + 1;
                let prev_peak = pk.load(Ordering::SeqCst);
                if cur > prev_peak {
                    pk.store(cur, Ordering::SeqCst);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
                inf.fetch_sub(1, Ordering::SeqCst);
                fast_work(i)
            }
        })
        .await;
    assert!(!samples.is_empty());
    assert!(
        peak.load(Ordering::SeqCst) >= 2,
        "expected peak in-flight >= 2 with 4 workers and 20ms work; got {}",
        peak.load(Ordering::SeqCst)
    );
}
