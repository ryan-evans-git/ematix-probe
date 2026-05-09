//! End-to-end load-probe demo.
//!
//! Spins a tiny in-process httpbin-shaped server (responds to
//! `/200`, `/500`, `/sleep/<ms>`), then runs a `LoadPlan` against
//! `/200` covering all four v0.1 `LoadAssertion` variants:
//! `P99Under`, `ErrorRateBelow`, `ThroughputAbove`, `StatusCodeIn`.
//!
//! Run from the repo root:
//!
//!     cargo run --example load_probe_demo --package ematix-probe-core
//!
//! No external network required — the server binds to
//! 127.0.0.1:0 (kernel picks an unused port) and the demo
//! tears it down at exit.

use std::time::Duration;

use ematix_probe_core::adapters::load::http::HttpLoadAdapter;
use ematix_probe_core::engine::data::Verdict;
use ematix_probe_core::engine::load::{
    evaluate_load, HttpTarget, LoadAssertion, LoadMode, LoadPlan,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = spawn_httpbin().await;
    println!("Local httpbin-shaped server: {url}");

    let plan = LoadPlan {
        target: HttpTarget::get(format!("{url}200")),
        duration: Duration::from_secs(2),
        mode: LoadMode::ConstantRate { rps: 25.0 },
        warmup: Duration::from_millis(100),
        assertions: vec![
            LoadAssertion::P99Under {
                metric: "latency_ms".into(),
                threshold_ms: 50.0,
            },
            LoadAssertion::ErrorRateBelow { threshold: 0.05 },
            LoadAssertion::ThroughputAbove {
                threshold_rps: 20.0,
            },
            LoadAssertion::StatusCodeIn { allowed: vec![200] },
        ],
    };

    let rps = plan.nominal_rps().unwrap_or(0.0);
    println!(
        "Driving {} req/s for {:?} (warmup {:?})...",
        rps, plan.duration, plan.warmup,
    );
    let adapter = HttpLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await?;

    println!("Collected {} samples; evaluating.\n", samples.len());
    let summary = evaluate_load(&plan, &samples);

    println!("Verdict: {:?}", summary.verdict);
    println!("Assertions:");
    for r in &summary.assertions {
        let marker = match r.verdict {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Error => " ERR",
        };
        let kind_label = match plan.assertions.get(r.assertion_index) {
            Some(LoadAssertion::P99Under { .. }) => "p99_under",
            Some(LoadAssertion::ErrorRateBelow { .. }) => "error_rate_below",
            Some(LoadAssertion::ThroughputAbove { .. }) => "throughput_above",
            Some(LoadAssertion::StatusCodeIn { .. }) => "status_code_in",
            _ => "unknown",
        };
        let line = format!("  [{marker}] {kind_label}");
        if let Some(msg) = &r.message {
            println!("{line}  -- {msg}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}

/// Tiny tokio TCP responder: parses the first request line for
/// the path and returns 200 / 500 / a configurable sleep.
async fn spawn_httpbin() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let mut total = 0;
                loop {
                    match sock.read(&mut buf[total..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            total += n;
                            if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                            if total == buf.len() {
                                break;
                            }
                        }
                    }
                }
                let req = String::from_utf8_lossy(&buf[..total]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, sleep_ms): (u16, u64) = if path.starts_with("/sleep/") {
                    let ms: u64 = path.trim_start_matches("/sleep/").parse().unwrap_or(0);
                    (200, ms)
                } else if path == "/500" {
                    (500, 0)
                } else {
                    (200, 0)
                };
                if sleep_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                }
                let body = b"ok\n";
                let resp = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.write_all(body).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    url
}
