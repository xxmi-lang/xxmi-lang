//! The small routes: service info, health, and the fallback.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::json;

use crate::VERSION;
use crate::app::AppState;
use crate::mcp::{self, json_response};

pub async fn index() -> Response {
    let body =
        json!({ "name": "xxmi-mcp", "version": VERSION, "mcp": "/mcp", "health": "/healthz" });
    json_response(StatusCode::OK, &body)
}

pub async fn healthz(State(state): State<Arc<AppState>>) -> Response {
    let body = json!({ "status": "ok", "ready": state.ctx.catalog.ready(), "version": VERSION });
    json_response(StatusCode::OK, &body)
}

pub async fn not_found() -> Response {
    json_response(StatusCode::NOT_FOUND, &json!({ "error": "not found" }))
}

pub async fn mcp_endpoint(State(state): State<Arc<AppState>>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    mcp::handle(&state, &parts.method, &parts.headers, body).await
}
