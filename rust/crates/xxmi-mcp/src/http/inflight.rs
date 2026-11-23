//! A cap on concurrent requests: past it a request is turned away at once, not queued.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::app::AppState;

const RETRY_AFTER_SEC: u64 = 1;

fn busy() -> Response {
    let body = json!({ "error": "server busy", "retryAfterSec": RETRY_AFTER_SEC });
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [
            (header::RETRY_AFTER, RETRY_AFTER_SEC.to_string()),
            (header::CONTENT_TYPE, "application/json".into()),
        ],
        body.to_string(),
    )
        .into_response()
}

pub async fn shed(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let Ok(_permit) = state.inflight.clone().try_acquire_owned() else {
        let path = request.uri().path();
        state.ctx.logger.warn("overloaded", json!({ "path": path }));
        return busy();
    };
    next.run(request).await
}
