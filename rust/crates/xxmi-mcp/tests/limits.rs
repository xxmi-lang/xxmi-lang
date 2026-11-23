//! Overload behaviour: load shedding, timeouts, the bounded package cache and idle connections.

mod support;

use std::future::pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use support::{app_with_state, mcp_request, send};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tower::ServiceExt;
use xxmi_mcp::cancel::cancelled;
use xxmi_mcp::catalog::Catalog;
use xxmi_mcp::logger::Logger;
use xxmi_mcp::packages::PackageCache;
use xxmi_mcp::repos::game_repo;
use xxmi_mcp::serve::serve;
use xxmi_mcp::store::Store;
use xxmi_mcp::tools::{BlockingGate, ToolError, ToolOutput, ToolResult, run_job, with_timeout};

fn ping() -> Request<Body> {
    mcp_request(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }))
}

#[tokio::test]
async fn a_full_inflight_cap_answers_503_with_retry_after() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_with_state(dir.path(), &[("MAX_INFLIGHT", "1")]).await;
    let held = state.inflight.clone().try_acquire_owned().unwrap();

    let shed = app.clone().oneshot(ping()).await.unwrap();
    let health = send(&app, Request::get("/healthz").body(Body::empty()).unwrap()).await;
    drop(held);
    let served = send(&app, ping()).await;

    assert_eq!(shed.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(shed.headers()[header::RETRY_AFTER], "1");
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(served.status, StatusCode::OK);
}

#[tokio::test]
async fn a_full_blocking_queue_answers_a_json_rpc_error() {
    let dir = tempfile::tempdir().unwrap();
    let (app, state) = app_with_state(dir.path(), &[]).await;
    let mut held = Vec::new();
    while let Ok(permit) = state.ctx.blocking.admit() {
        held.push(permit);
    }

    let body = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": "spec_lookup", "arguments": { "name": "match_dhash" } } });
    let reply = send(&app, mcp_request(&body)).await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["error"]["code"], -32000);
    assert!(
        reply.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("busy")
    );
}

#[tokio::test]
async fn a_call_past_the_tool_timeout_answers_the_timeout_error() {
    let slow = with_timeout("lint_ini", Duration::from_millis(50), pending()).await;

    let error = slow.err().expect("the call times out");

    assert_eq!(error.message, "lint_ini timed out after 50 ms");
    assert!(error.expected);
}

fn done() -> ToolResult {
    Ok(ToolOutput {
        text: "done".into(),
        data: json!({}),
    })
}

/// A tool that works until its call times out, as a long scan does between its checkpoints.
fn work_until_cancelled() -> ToolResult {
    while !cancelled() {
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(ToolError::new("cancelled"))
}

fn runtime_with_threads(threads: usize) -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(threads)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn timed_out_work_releases_its_permits_once_it_sees_the_cancellation() {
    runtime_with_threads(2).block_on(async {
        let gate = BlockingGate::new(2);
        let call = || {
            let permit = gate.admit().unwrap();
            with_timeout(
                "slow",
                Duration::from_millis(50),
                run_job(permit, work_until_cancelled),
            )
        };

        let timeouts = tokio::join!(call(), call());
        let full = gate.admit().err().map(|e| e.busy);
        let started = Instant::now();
        let permit = loop {
            match gate.admit() {
                Ok(permit) => break permit,
                Err(_) => {
                    assert!(started.elapsed() < Duration::from_secs(1), "never freed");
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        };
        let served = run_job(permit, done).await;

        assert!(timeouts.0.is_err() && timeouts.1.is_err());
        assert_eq!(full, Some(true));
        assert!(served.is_ok());
    });
}

#[test]
fn a_task_that_times_out_in_the_queue_never_runs() {
    runtime_with_threads(1).block_on(async {
        let gate = BlockingGate::new(2);
        let release = Arc::new(AtomicBool::new(false));
        let ran = Arc::new(AtomicBool::new(false));

        let blocker = {
            let release = release.clone();
            run_job(gate.admit().unwrap(), move || {
                while !release.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                done()
            })
        };
        let blocker = tokio::spawn(blocker);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let queued = {
            let ran = ran.clone();
            run_job(gate.admit().unwrap(), move || {
                ran.store(true, Ordering::Relaxed);
                done()
            })
        };
        let timed_out = with_timeout("queued", Duration::from_millis(50), queued).await;
        release.store(true, Ordering::Relaxed);
        blocker.await.unwrap().unwrap();
        // The queued task's turn comes right after the blocker ends; give it room to wrongly run.
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(timed_out.is_err());
        assert!(!ran.load(Ordering::Relaxed));
        assert!(gate.admit().is_ok());
    });
}

#[tokio::test]
async fn the_package_cache_keeps_at_most_its_capacity() {
    let dir = tempfile::tempdir().unwrap();
    support::make_volume(dir.path());
    let catalog = Catalog::load(Store::new(dir.path())).await;
    let repo = game_repo("zzmi").unwrap();
    let (head, tag) = (
        catalog.resolve(repo, Some("main")).unwrap(),
        catalog.resolve(repo, Some("v1.0.0")).unwrap(),
    );
    let cache = PackageCache::new(1);

    let first = cache.snapshot(&head, None).unwrap();
    let again = cache.snapshot(&head, None).unwrap();
    cache.snapshot(&tag, None).unwrap();
    let rebuilt = cache.snapshot(&head, None).unwrap();

    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(cache.len(), 1);
    assert!(!std::sync::Arc::ptr_eq(&first, &rebuilt));
}

#[tokio::test]
async fn a_silent_connection_is_closed_after_the_header_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app_with_state(dir.path(), &[]).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        app,
        Duration::from_millis(200),
        Logger::default(),
        pending(),
    ));

    let mut silent = TcpStream::connect(addr).await.unwrap();
    let mut buffer = [0u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(5), silent.read(&mut buffer)).await;

    server.abort();
    assert!(matches!(read, Ok(Ok(0)) | Ok(Err(_))), "{read:?}");
}

#[tokio::test]
async fn an_idle_keep_alive_connection_is_closed_after_the_idle_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app_with_state(dir.path(), &[]).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        app,
        Duration::from_millis(200),
        Logger::default(),
        pending(),
    ));

    let mut client = TcpStream::connect(addr).await.unwrap();
    client
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut seen = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut seen)).await;

    server.abort();
    assert!(String::from_utf8_lossy(&seen).starts_with("HTTP/1.1 200"));
    assert!(matches!(closed, Ok(Ok(_)) | Ok(Err(_))), "{closed:?}");
}
