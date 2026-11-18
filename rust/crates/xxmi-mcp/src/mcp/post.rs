//! The HTTP layer of `/mcp`: method, body, headers and batches, before any request is dispatched.

use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::app::AppState;
use crate::jsonrpc::{
    INVALID_REQUEST, Message, PARSE_ERROR, Request, SERVER_ERROR, classify, transport_error,
};

use super::initialize::{SUPPORTED_VERSIONS, params_issues};
use super::methods::dispatch;

const MAX_BATCH: usize = 100;

pub fn json_response(status: StatusCode, body: &Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn rejection(status: StatusCode, code: i64, message: &str) -> Response {
    json_response(status, &transport_error(code, message))
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn accepts_both(headers: &HeaderMap) -> bool {
    let accept = header_text(headers, "accept").unwrap_or_default();
    accept.contains("application/json") && accept.contains("text/event-stream")
}

fn is_json_content(headers: &HeaderMap) -> bool {
    header_text(headers, "content-type").is_some_and(|v| v.contains("application/json"))
}

fn unsupported_version(headers: &HeaderMap) -> Option<String> {
    let version = header_text(headers, "mcp-protocol-version")?;
    (!SUPPORTED_VERSIONS.contains(&version)).then(|| {
        format!(
            "Bad Request: Unsupported protocol version: {version} (supported versions: {})",
            SUPPORTED_VERSIONS.join(", ")
        )
    })
}

fn is_initialize(message: &Message) -> bool {
    matches!(message, Message::Request(r) if r.method == "initialize" && params_issues(r.params.as_ref()).is_empty())
}

/// Checks the headers and the message shapes; the error is the response to send.
// The rejection is returned as-is to the client, so boxing it would only add an allocation.
#[allow(clippy::result_large_err)]
fn admit(headers: &HeaderMap, body: &Value) -> Result<Vec<Message>, Response> {
    if !accepts_both(headers) {
        return Err(rejection(
            StatusCode::NOT_ACCEPTABLE,
            SERVER_ERROR,
            "Not Acceptable: Client must accept both application/json and text/event-stream",
        ));
    }
    if !is_json_content(headers) {
        return Err(rejection(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            SERVER_ERROR,
            "Unsupported Media Type: Content-Type must be application/json",
        ));
    }

    let items: Vec<&Value> = match body {
        Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };
    if items.len() > MAX_BATCH {
        return Err(rejection(
            StatusCode::BAD_REQUEST,
            INVALID_REQUEST,
            "Invalid Request: Batch must not exceed 100 messages",
        ));
    }

    let messages: Option<Vec<Message>> = items.into_iter().map(classify).collect();
    let Some(messages) = messages else {
        return Err(rejection(
            StatusCode::BAD_REQUEST,
            PARSE_ERROR,
            "Parse error: Invalid JSON-RPC message",
        ));
    };

    if messages.iter().any(is_initialize) {
        if messages.len() > 1 {
            return Err(rejection(
                StatusCode::BAD_REQUEST,
                INVALID_REQUEST,
                "Invalid Request: Only one initialization request is allowed",
            ));
        }
    } else if let Some(message) = unsupported_version(headers) {
        return Err(rejection(StatusCode::BAD_REQUEST, SERVER_ERROR, &message));
    }
    Ok(messages)
}

async fn answer(state: &AppState, messages: Vec<Message>) -> Response {
    let requests: Vec<Request> = messages
        .into_iter()
        .filter_map(|m| {
            if let Message::Request(r) = m {
                Some(r)
            } else {
                None
            }
        })
        .collect();

    let mut replies = Vec::with_capacity(requests.len());
    for request in &requests {
        replies.push(dispatch(state, request).await);
    }
    match replies.len() {
        0 => StatusCode::ACCEPTED.into_response(),
        1 => json_response(StatusCode::OK, &replies[0]),
        _ => json_response(StatusCode::OK, &Value::Array(replies)),
    }
}

pub async fn handle(
    state: &AppState,
    method: &Method,
    headers: &HeaderMap,
    body: Body,
) -> Response {
    if method != Method::POST {
        let mut response = rejection(
            StatusCode::METHOD_NOT_ALLOWED,
            SERVER_ERROR,
            "Method not allowed: this server is stateless, use POST.",
        );
        response
            .headers_mut()
            .insert(header::ALLOW, "POST".parse().expect("static header value"));
        return response;
    }

    let limit = state.ctx.config.max_body_bytes;
    let Ok(bytes) = to_bytes(body, limit).await else {
        return rejection(
            StatusCode::PAYLOAD_TOO_LARGE,
            INVALID_REQUEST,
            &format!("Request body exceeds {limit} bytes."),
        );
    };
    let Ok(parsed) = serde_json::from_slice::<Value>(&bytes) else {
        return rejection(
            StatusCode::BAD_REQUEST,
            PARSE_ERROR,
            "Parse error: body is not valid JSON.",
        );
    };

    match admit(headers, &parsed) {
        Ok(messages) => answer(state, messages).await,
        Err(response) => response,
    }
}
