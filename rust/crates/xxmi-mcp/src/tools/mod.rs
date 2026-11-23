//! The nine tools: argument validation against the published schemas, dispatch, and the timeout.

mod blocking;
mod library;
mod lint;
mod package;
mod shared;
mod source;
mod spec_tools;
mod versions;

use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use serde_json::{Map, Value};

pub use blocking::{BlockingGate, MAX_BLOCKING_THREADS, run_job};
pub use shared::{Args, ToolContext, ToolError, ToolOutput, ToolResult};

use crate::schema;

use blocking::run_blocking;

pub const TOOLS_LIST_JSON: &str = include_str!("../protocol/tools_list.json");

static TOOLS_LIST: LazyLock<Vec<Value>> = LazyLock::new(|| {
    serde_json::from_str(TOOLS_LIST_JSON).expect("the embedded tool list is valid JSON")
});

fn definition(name: &str) -> Option<&'static Value> {
    TOOLS_LIST
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
}

pub fn tools_list() -> &'static [Value] {
    &TOOLS_LIST
}

pub fn is_tool(name: &str) -> bool {
    definition(name).is_some()
}

/// Checks and defaults `arguments` by the tool's input schema;
/// the error is the issue text the protocol layer reports.
pub fn validate_arguments(name: &str, arguments: &Map<String, Value>) -> Result<Args, Vec<String>> {
    let schema = definition(name)
        .and_then(|tool| tool.get("inputSchema"))
        .cloned()
        .unwrap_or(Value::Null);
    schema::validate(&schema, arguments).map(Args)
}

type SyncTool = fn(&ToolContext, &Args) -> ToolResult;

fn sync_tool(name: &str) -> Option<SyncTool> {
    Some(match name {
        "spec_lookup" => spec_tools::spec_lookup,
        "spec_list" => spec_tools::spec_list,
        "library_list" => library::library_list,
        "library_symbol" => library::library_symbol,
        "find_implementation" => source::find_implementation_tool,
        "read_source" => source::read_source_tool,
        "lint_ini" => lint::lint_ini,
        "list_versions" => |ctx, _| versions::list_versions(ctx),
        _ => return None,
    })
}

async fn dispatch(ctx: Arc<ToolContext>, name: &str, args: Args) -> ToolResult {
    match sync_tool(name) {
        Some(run) => run_blocking(ctx, run, args).await,
        None => source::search_source_tool(&ctx, &args).await,
    }
}

/// Answers with an error once `limit` has passed. The dropped call stops waiting on its work:
/// a queued blocking task is skipped, a subprocess is killed (`kill_on_drop`),
/// and a running one finishes on its own.
pub async fn with_timeout(
    name: &str,
    limit: Duration,
    call: impl Future<Output = ToolResult>,
) -> ToolResult {
    match tokio::time::timeout(limit, call).await {
        Ok(result) => result,
        Err(_) => Err(ToolError::new(format!(
            "{name} timed out after {} ms",
            limit.as_millis()
        ))),
    }
}

/// Runs a validated call within the tool timeout.
pub async fn execute(ctx: Arc<ToolContext>, name: &str, args: Args) -> ToolResult {
    let limit = Duration::from_millis(ctx.config.tool_timeout_ms);
    with_timeout(name, limit, dispatch(ctx, name, args)).await
}
