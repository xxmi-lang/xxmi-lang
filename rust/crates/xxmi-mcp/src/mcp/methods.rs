//! One JSON-RPC request to its response: the methods this stateless server answers.

use std::time::Instant;

use serde_json::{Value, json};

use crate::app::AppState;
use crate::jsonrpc::{INTERNAL_ERROR, METHOD_NOT_FOUND, Request, failure, success};
use crate::tools::{self, ToolError};
use crate::zod::{Issue, issues_message};

use super::{initialize, params, resources};

fn invalid_params(id: &Value, issues: &[Issue]) -> Value {
    failure(id, INTERNAL_ERROR, &issues_message(issues))
}

/// The SDK re-serializes every tool result in its schema order, so `content` comes first.
fn text_result(text: String, is_error: bool) -> Value {
    let content = json!([{ "type": "text", "text": text }]);
    if is_error {
        json!({ "content": content, "isError": true })
    } else {
        json!({ "content": content })
    }
}

fn list_of(json_text: &str, key: &str) -> Value {
    let items: Value = serde_json::from_str(json_text).unwrap_or(Value::Null);
    json!({ key: items })
}

/// The caller sees the message of an expected failure; anything else stays in the log.
fn failure_text(name: &str, error: &ToolError) -> String {
    if error.expected {
        error.message.clone()
    } else {
        format!("{name} failed: internal error")
    }
}

async fn call_tool(state: &AppState, request: &Request) -> Value {
    let call = match params::tool_call(request.params.as_ref()) {
        Ok(call) => call,
        Err(issues) => return invalid_params(&request.id, &issues),
    };
    let name = call.name;
    if !tools::is_tool(&name) {
        return success(
            &request.id,
            text_result(format!("MCP error -32602: Tool {name} not found"), true),
        );
    }
    let args = match tools::validate_arguments(&name, &call.arguments) {
        Ok(args) => args,
        Err(messages) => {
            let text = format!(
                "MCP error -32602: Input validation error: Invalid arguments for tool {name}: {}",
                messages.join("\n")
            );
            return success(&request.id, text_result(text, true));
        }
    };

    let as_json = args.wants_json();
    let started = Instant::now();
    let outcome = tools::execute(state.ctx.clone(), &name, args).await;
    let ms = started.elapsed().as_millis() as u64;
    let logger = &state.ctx.logger;

    match outcome {
        Ok(output) => {
            logger.info("tool", json!({ "tool": name, "ms": ms, "ok": true }));
            let text = if as_json {
                serde_json::to_string_pretty(&output.data).unwrap_or_default()
            } else {
                output.text
            };
            success(&request.id, text_result(text, false))
        }
        Err(error) => {
            logger.info(
                "tool",
                json!({ "tool": name, "ms": ms, "ok": false, "expected": error.expected }),
            );
            if !error.expected {
                logger.error(
                    "tool failed",
                    json!({ "tool": name, "error": error.message }),
                );
            }
            success(&request.id, text_result(failure_text(&name, &error), true))
        }
    }
}

fn read_resource(state: &AppState, request: &Request) -> Value {
    let uri = match params::resource_uri(request.params.as_ref()) {
        Ok(uri) => uri,
        Err(issues) => return invalid_params(&request.id, &issues),
    };
    match resources::read(&state.ctx, &uri) {
        Ok(result) => success(&request.id, result),
        Err((code, message)) => failure(&request.id, code, &message),
    }
}

fn initialize(request: &Request) -> Value {
    let issues = initialize::params_issues(request.params.as_ref());
    match (issues.is_empty(), &request.params) {
        (true, Some(params)) => success(&request.id, initialize::result(params)),
        _ => invalid_params(&request.id, &issues),
    }
}

pub async fn dispatch(state: &AppState, request: &Request) -> Value {
    let id = &request.id;
    match request.method.as_str() {
        "initialize" => initialize(request),
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({ "tools": tools::tools_list() })),
        "tools/call" => call_tool(state, request).await,
        "resources/list" => success(id, list_of(resources::RESOURCES_LIST_JSON, "resources")),
        "resources/templates/list" => {
            success(id, list_of(resources::TEMPLATES_JSON, "resourceTemplates"))
        }
        "resources/read" => read_resource(state, request),
        _ => failure(id, METHOD_NOT_FOUND, "Method not found"),
    }
}
