//! Transport behaviour: health, body limit, rate limiting and admin auth.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use support::{app, mcp_request, send};

fn from(forwarded: &str) -> Request<Body> {
    let mut request = mcp_request(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }));
    request
        .headers_mut()
        .insert("x-forwarded-for", forwarded.parse().unwrap());
    request
}

#[tokio::test]
async fn healthz_reports_ready() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let reply = send(&app, Request::get("/healthz").body(Body::empty()).unwrap()).await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["status"], "ok");
    assert_eq!(reply.body["ready"], true);
}

#[tokio::test]
async fn an_oversized_body_is_a_413() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[("MAX_BODY_BYTES", "1024")]).await;
    let big = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": { "pad": "x".repeat(2048) } });

    let reply = send(&app, mcp_request(&big)).await;

    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(reply.body["error"]["code"], -32600);
}

#[tokio::test]
async fn the_rate_limit_keys_on_the_nth_forwarded_entry_from_the_right() {
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("RATE_LIMIT_PER_MIN", "1"),
        ("RATE_LIMIT_BURST", "1"),
        ("TRUSTED_PROXY_HOPS", "1"),
    ];
    let app = app(dir.path(), &env).await;

    let first = send(&app, from("1.1.1.1")).await;
    let second = send(&app, from("1.1.1.1")).await;
    let other = send(&app, from("2.2.2.2")).await;

    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(second.body["error"], "rate limit exceeded");
    assert_eq!(other.status, StatusCode::OK);
}

#[tokio::test]
async fn a_spoofed_leftmost_entry_does_not_dodge_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("RATE_LIMIT_PER_MIN", "1"),
        ("RATE_LIMIT_BURST", "1"),
        ("TRUSTED_PROXY_HOPS", "1"),
    ];
    let app = app(dir.path(), &env).await;

    let first = send(&app, from("6.6.6.6, 1.1.1.1")).await;
    let second = send(&app, from("7.7.7.7, 1.1.1.1")).await;

    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn admin_refresh_is_hidden_without_a_token_and_guarded_with_one() {
    let (open_dir, guarded_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let open = app(open_dir.path(), &[]).await;
    let guarded = app(guarded_dir.path(), &[("ADMIN_TOKEN", "s3cret")]).await;
    let post = |auth: Option<&str>| {
        let mut request = Request::post("/admin/refresh");
        if let Some(auth) = auth {
            request = request.header("authorization", auth);
        }
        request.body(Body::empty()).unwrap()
    };

    let hidden = send(&open, post(None)).await;
    let missing = send(&guarded, post(None)).await;
    let wrong = send(&guarded, post(Some("Bearer nope"))).await;
    let right = send(&guarded, post(Some("Bearer s3cret"))).await;

    assert_eq!(hidden.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert!(right.status.is_success(), "{}", right.body);
}
