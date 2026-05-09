//! S-8.3 — `HttpLoadAdapter` under `LoadMode::VirtualUsers`.
//!
//! Reuses the in-process tokio TCP responder pattern from
//! `http_load_adapter.rs`; the difference is the LoadMode
//! variant. Same Sample shape; the count of samples depends on
//! per-request response time (closed model) rather than a
//! configured RPS (open model).

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ematix_probe_core::adapters::load::http::HttpLoadAdapter;
use ematix_probe_core::engine::load::{HttpTarget, LoadMode, LoadPlan};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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
async fn virtual_users_mode_collects_samples_via_vu_pool() {
    let status = Arc::new(AtomicU16::new(200));
    let url = spawn_canned_server(status).await;
    let plan = LoadPlan {
        target: HttpTarget::get(&url),
        duration: Duration::from_millis(500),
        mode: LoadMode::VirtualUsers { count: 4 },
        warmup: Duration::ZERO,
        assertions: vec![],
    };
    let adapter = HttpLoadAdapter::new();
    let samples = adapter.collect_samples(&plan).await.expect("collect");

    // 4 workers, ~500ms, very fast localhost requests → expect
    // tens of samples. Lower bound is loose: each worker must
    // start at least once.
    assert!(
        samples.len() >= 4,
        "expected >= 4 samples (one per worker); got {}",
        samples.len()
    );
    for s in &samples {
        assert_eq!(s.status_code, Some(200));
    }
    // tick_index should be unique + monotonically assigned
    let mut indices: Vec<u64> = samples.iter().map(|s| s.tick_index).collect();
    indices.sort();
    let n_before = indices.len();
    indices.dedup();
    assert_eq!(indices.len(), n_before, "tick_index collisions");
}
