//! Request parameters of `tools/call` and `resources/read`, checked like the SDK's schemas.

use serde_json::{Map, Value};

use crate::zod::{Issue, check, invalid_type};

pub struct ToolCall {
    pub name: String,
    pub arguments: Map<String, Value>,
}

fn params_object(params: Option<&Value>) -> Result<&Map<String, Value>, Vec<Issue>> {
    match check("object", params, &["params"]) {
        Some(issue) => Err(vec![issue]),
        None => Ok(params
            .and_then(Value::as_object)
            .expect("checked to be an object")),
    }
}

pub fn tool_call(params: Option<&Value>) -> Result<ToolCall, Vec<Issue>> {
    let object = params_object(params)?;
    let mut issues: Vec<Issue> = Vec::new();
    issues.extend(check("string", object.get("name"), &["params", "name"]));

    let arguments = match object.get("arguments") {
        None => Some(Map::new()),
        Some(Value::Object(map)) => Some(map.clone()),
        Some(other) => {
            issues.push(invalid_type(
                "record",
                Some(other),
                &["params", "arguments"],
            ));
            None
        }
    };
    match (issues.is_empty(), arguments) {
        (true, Some(arguments)) => {
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(ToolCall { name, arguments })
        }
        _ => Err(issues),
    }
}

pub fn resource_uri(params: Option<&Value>) -> Result<String, Vec<Issue>> {
    let object = params_object(params)?;
    match check("string", object.get("uri"), &["params", "uri"]) {
        Some(issue) => Err(vec![issue]),
        None => Ok(object
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()),
    }
}
