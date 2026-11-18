//! `POST /admin/refresh`: starts an indexing pass, behind a bearer token.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::Response;
use serde_json::json;

use crate::app::AppState;
use crate::mcp::json_response;

use super::routes::not_found;

/// Compares without stopping at the first differing byte, so timing does not leak the token.
fn same_token(given: &[u8], expected: &[u8]) -> bool {
    let mut diff = given.len() ^ expected.len();
    for (a, b) in given.iter().zip(expected) {
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

/// The token after `Bearer` (any case), with its whitespace stripped.
fn bearer(headers: &HeaderMap) -> &str {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    match value.get(..6) {
        Some(scheme)
            if scheme.eq_ignore_ascii_case("bearer")
                && value[6..].starts_with(char::is_whitespace) =>
        {
            value[6..].trim_start()
        }
        _ => value,
    }
}

fn wants_wait(request: &Request) -> bool {
    request
        .uri()
        .query()
        .is_some_and(|q| q.split('&').any(|pair| pair == "wait=1"))
}

pub async fn refresh(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let Some(token) = state.ctx.config.admin_token.as_deref() else {
        return not_found().await;
    };
    if request.method() != Method::POST {
        let mut response = json_response(
            StatusCode::METHOD_NOT_ALLOWED,
            &json!({ "error": "use POST" }),
        );
        response
            .headers_mut()
            .insert(header::ALLOW, "POST".parse().expect("static header value"));
        return response;
    }
    if !same_token(bearer(request.headers()).as_bytes(), token.as_bytes()) {
        let body = json!({ "error": "invalid or missing bearer token" });
        return json_response(StatusCode::UNAUTHORIZED, &body);
    }

    if wants_wait(&request) {
        let result = state.indexer.refresh().await;
        return json_response(
            StatusCode::OK,
            &serde_json::to_value(result).unwrap_or_default(),
        );
    }

    let indexer = state.indexer.clone();
    tokio::spawn(async move {
        indexer.refresh().await;
    });
    json_response(StatusCode::ACCEPTED, &json!({ "status": "started" }))
}
