//! `initialize`: protocol negotiation and the server's capabilities.

use serde_json::{Value, json};

use crate::VERSION;
use crate::zod::{Issue, check};

pub const SUPPORTED_VERSIONS: [&str; 5] = [
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
    "2024-10-07",
];
const LATEST_VERSION: &str = SUPPORTED_VERSIONS[0];

const INSTRUCTIONS: &str = "Read-only lookups for 3DMigoto/XXMI mod ini files: spec_lookup/spec_list for sections, keys and commands; library_list/library_symbol for game-package libraries (e.g. CommandList\\ZZMI\\SetTextures); find_implementation/search_source/read_source for the XXMI DLL and package source; lint_ini to lint ini text.";

/// What the SDK's `InitializeRequest` schema rejects, in its field order.
pub fn params_issues(params: Option<&Value>) -> Vec<Issue> {
    if let Some(issue) = check("object", params, &["params"]) {
        return vec![issue];
    }

    let params = params.and_then(Value::as_object);
    let field = |name: &str| params.and_then(|p| p.get(name));
    let mut issues: Vec<Issue> = Vec::new();
    issues.extend(check(
        "string",
        field("protocolVersion"),
        &["params", "protocolVersion"],
    ));
    issues.extend(check(
        "object",
        field("capabilities"),
        &["params", "capabilities"],
    ));

    match check("object", field("clientInfo"), &["params", "clientInfo"]) {
        Some(issue) => issues.push(issue),
        None => {
            let info = field("clientInfo").and_then(Value::as_object);
            for name in ["name", "version"] {
                let value = info.and_then(|i| i.get(name));
                issues.extend(check("string", value, &["params", "clientInfo", name]));
            }
        }
    }
    issues
}

pub fn result(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let version = if SUPPORTED_VERSIONS.contains(&requested) {
        requested
    } else {
        LATEST_VERSION
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": true }, "resources": { "listChanged": true } },
        "serverInfo": { "name": "xxmi-mcp", "version": VERSION },
        "instructions": INSTRUCTIONS,
    })
}
