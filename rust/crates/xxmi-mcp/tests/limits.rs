//! Overload behaviour: load shedding, timeouts, the bounded package cache and idle connections.

mod support;

use std::future::pending;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use support::{app_with_state, mcp_request, send};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tower::ServiceExt;
use xxmi_mcp::catalog::Catalog;
use xxmi_mcp::logger::Logger;
use xxmi_mcp::packages::PackageCache;
use xxmi_mcp::repos::game_repo;
use xxmi_mcp::serve::serve;
use xxmi_mcp::store::Store;
use xxmi_mcp::tools::with_timeout;

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
