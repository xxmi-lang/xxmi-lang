//! Per-client request budget in front of `/mcp` and `/admin/refresh`.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::app::AppState;

use super::client_ip::{client_ip, socket_ip};

pub async fn limit(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let socket = socket_ip(&request);
    let ip = client_ip(
        request.headers(),
        socket.as_deref(),
        state.ctx.config.trusted_proxy_hops,
    );
    let decision = state
        .limiter
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take(&ip);
    if decision.allowed {
        return next.run(request).await;
    }

    let retry = decision.retry_after_sec;
    state.ctx.logger.warn(
        "rate limited",
        json!({ "ip": ip, "path": request.uri().path(), "retryAfterSec": retry }),
    );
    let body = json!({ "error": "rate limit exceeded", "retryAfterSec": retry });
    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            (header::RETRY_AFTER, retry.to_string()),
            (header::CONTENT_TYPE, "application/json".into()),
        ],
        body.to_string(),
    )
        .into_response()
}
