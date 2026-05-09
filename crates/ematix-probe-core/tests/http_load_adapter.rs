//! S-6.5 — `HttpLoadAdapter`: drives the constant-rate
//! scheduler against an HTTP target, collects per-tick samples.
//!
//! Tests use a tiny in-process tokio TCP server that returns
//! a canned HTTP response — no extra crate deps, no network.
//! The server's response status is configurable per test so the
//! sample collection path is exercised across 200 / 4xx / 5xx.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ematix_probe_core::adapters::load::http::HttpLoadAdapter;
use ematix_probe_core::engine::load::{HttpTarget, LoadMode, LoadPlan};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Spin a tiny HTTP server on 127.0.0.1:0; return its base URL.
/// Every accepted connection reads up to the request boundary
/// then writes back a canned response with the configured status.
async fn spawn_canned_server(status: Arc<AtomicU16>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => continue,
            };
            let status = status.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                // Read until we see "\r\n\r\n" (end of HTTP
                // request headers). Tolerant of partial reads.
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
                let s = status.load(Ordering::SeqCst);
                let body = b"hi\n";
                let resp = format!(
                    "HTTP/1.1 {s} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

#[tokio::test]
async fn collects_one_sample_per_tick() {
    let status = Arc::new(AtomicU16::new(200));
    let url = spawn_canned_server(status.clone()).await;

    let plan = LoadPlan {
        target: HttpTarget::get(&url),
        duration: Duration::from_millis(500),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = HttpLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");

    // 10 RPS for 500ms → 5 ticks (boundary excluded).
    assert!(
        (4..=6).contains(&samples.len()),
        "10 RPS / 500ms expected ~5 samples; got {}",
        samples.len()
    );
    for s in &samples {
        assert_eq!(s.status_code, Some(200), "expected 200 OK; got {s:?}");
        assert!(s.error.is_none());
        assert!(s.latency.as_millis() < 500, "latency seems off: {s:?}");
    }
}

#[tokio::test]
async fn surfaces_5xx_status_codes() {
    let status = Arc::new(AtomicU16::new(503));
    let url = spawn_canned_server(status).await;

    let plan = LoadPlan {
        target: HttpTarget::get(&url),
        duration: Duration::from_millis(300),
        mode: LoadMode::ConstantRate { rps: 10.0 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = HttpLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");

    assert!(!samples.is_empty());
    for s in &samples {
        assert_eq!(s.status_code, Some(503));
        assert!(
            s.error.is_none(),
            "5xx is a successful round-trip; status carries the failure"
        );
    }
}

#[tokio::test]
async fn unreachable_target_yields_error_samples() {
    // Port 1 should refuse — no real listener. Each tick should
    // produce a sample with `error: Some(_)`, no status_code.
    let plan = LoadPlan {
        target: HttpTarget::get("http://127.0.0.1:1/"),
        duration: Duration::from_millis(300),
        mode: LoadMode::ConstantRate { rps: 5.0 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = HttpLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");

    assert!(
        !samples.is_empty(),
        "unreachable target should still emit samples"
    );
    for s in &samples {
        assert!(s.status_code.is_none(), "no status on connection failure");
        assert!(s.error.is_some(), "expected error message");
    }
}
