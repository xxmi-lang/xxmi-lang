//! Shared state and the router.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::middleware::{from_fn_with_state, map_response};
use axum::response::Response;
use axum::routing::any;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

use crate::http::{admin, log, rate_limit, routes};
use crate::indexer::Indexer;
use crate::jsonrpc::{INTERNAL_ERROR, INVALID_REQUEST, transport_error};
use crate::mcp::json_response;
use crate::ratelimit::RateLimiter;
use crate::tools::ToolContext;

/// Slack over the tool timeout, so a slow tool answers with its own error first.
const REQUEST_TIMEOUT_SLACK_MS: u64 = 5_000;

pub struct AppState {
    pub ctx: Arc<ToolContext>,
    pub limiter: Mutex<RateLimiter>,
    pub indexer: Arc<dyn Indexer>,
}

impl AppState {
    pub fn new(ctx: ToolContext, indexer: Arc<dyn Indexer>) -> Arc<Self> {
        let limiter = RateLimiter::new(ctx.config.rate_limit_per_min, ctx.config.rate_limit_burst);
        Arc::new(Self {
            ctx: Arc::new(ctx),
            limiter: Mutex::new(limiter),
            indexer,
        })
    }
}

/// The limit layer answers an oversized `Content-Length` with an empty 413.
async fn json_too_large(max_body_bytes: usize, response: Response) -> Response {
    if response.status() != StatusCode::PAYLOAD_TOO_LARGE {
        return response;
    }
    let message = format!("Request body exceeds {max_body_bytes} bytes.");
    json_response(
        StatusCode::PAYLOAD_TOO_LARGE,
        &transport_error(INVALID_REQUEST, &message),
    )
}

fn internal_error() -> Response {
    json_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        &transport_error(INTERNAL_ERROR, "Internal server error"),
    )
}

pub fn router(state: Arc<AppState>) -> Router {
    let config = &state.ctx.config;
    let max_body = config.max_body_bytes;
    let timeout = Duration::from_millis(config.tool_timeout_ms + REQUEST_TIMEOUT_SLACK_MS);

    let limited = Router::new()
        .route("/mcp", any(routes::mcp_endpoint))
        .route("/admin/refresh", any(admin::refresh))
        .route_layer(from_fn_with_state(state.clone(), rate_limit::limit));

    Router::new()
        .route("/", any(routes::index))
        .route("/healthz", any(routes::healthz))
        .merge(limited)
        .fallback(routes::not_found)
        .layer(RequestBodyLimitLayer::new(max_body))
        .layer(map_response(move |response| {
            json_too_large(max_body, response)
        }))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
        .layer(CatchPanicLayer::custom(|_| internal_error()))
        .layer(from_fn_with_state(state.clone(), log::log_requests))
        .with_state(state)
}
