//! The `xxmi://spec` resources: the spec of a DLL version as JSON.

use serde_json::{Map, Value, json};

use crate::jsonrpc::{INTERNAL_ERROR, INVALID_PARAMS};
use crate::tools::ToolContext;

pub const RESOURCES_LIST_JSON: &str = include_str!("../protocol/resources_list.json");
pub const TEMPLATES_JSON: &str = include_str!("../protocol/resource_templates.json");

const SPEC_URI: &str = "xxmi://spec";

const SECTIONS: [&str; 3] = ["meta", "sections", "builtinSections"];
const COMMANDS: [&str; 4] = ["meta", "commands", "resourceMembers", "functions"];

pub type RpcFailure = (i64, String);

#[derive(Clone, Copy)]
enum SpecFile {
    Sections,
    Commands,
}

/// `(dll_ref, file)` of a spec URI; variables match `[^/]+` like the SDK's templates.
fn parse_uri(uri: &str) -> Option<(Option<&str>, Option<&str>)> {
    let tail = uri.strip_prefix(SPEC_URI)?;
    if tail.is_empty() {
        return Some((None, None));
    }

    let parts: Vec<&str> = tail.strip_prefix('/')?.split('/').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    match parts.as_slice() {
        [dll_ref] => Some((Some(dll_ref), None)),
        [dll_ref, file] => Some((Some(dll_ref), Some(file))),
        _ => None,
    }
}

fn spec_file(name: &str) -> Result<SpecFile, RpcFailure> {
    match name {
        "sections.json" => Ok(SpecFile::Sections),
        "commands.json" => Ok(SpecFile::Commands),
        _ => Err((INTERNAL_ERROR, format!("unknown spec file \"{name}\""))),
    }
}

fn document(doc: &Value, file: Option<SpecFile>) -> Map<String, Value> {
    let keys: Vec<&str> = match file {
        Some(SpecFile::Sections) => SECTIONS.to_vec(),
        Some(SpecFile::Commands) => COMMANDS.to_vec(),
        None => SECTIONS
            .iter()
            .chain(COMMANDS.iter().skip(1))
            .copied()
            .collect(),
    };
    keys.into_iter()
        .filter_map(|key| Some((key.to_string(), doc.get(key)?.clone())))
        .collect()
}

pub fn read(ctx: &ToolContext, uri: &str) -> Result<Value, RpcFailure> {
    let Some((dll_ref, file)) = parse_uri(uri) else {
        return Err((
            INVALID_PARAMS,
            format!("MCP error -32602: Resource {uri} not found"),
        ));
    };
    let file = file.map(spec_file).transpose()?;
    let spec = ctx
        .resolve_spec(dll_ref)
        .map_err(|e| (INTERNAL_ERROR, e.message))?;

    let mut body = document(&spec.doc, file);
    body.insert("_spec".into(), spec.block());
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": Value::Object(body).to_string(),
        }]
    }))
}
