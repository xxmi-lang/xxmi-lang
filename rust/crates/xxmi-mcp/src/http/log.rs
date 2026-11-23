//! One structured log line per request.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde_json::json;

use crate::app::AppState;

pub async fn log_requests(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let started = Instant::now();

    let response = next.run(request).await;
    state.ctx.logger.info(
        "http",
        json!({
            "method": method,
            "path": path,
            "status": response.status().as_u16(),
            "ms": started.elapsed().as_millis() as u64,
        }),
    );
    response
}
