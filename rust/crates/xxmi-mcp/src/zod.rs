//! The validation messages the TypeScript server's zod schemas produce, so error text matches.

use serde::Serialize;
use serde_json::Value;

/// One issue of a failed type check, in zod's field order.
#[derive(Debug, Serialize)]
pub struct Issue {
    pub expected: &'static str,
    pub code: &'static str,
    pub path: Vec<String>,
    pub message: String,
}

pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `undefined` is how zod names an absent value.
pub fn received(value: Option<&Value>) -> &'static str {
    value.map_or("undefined", type_name)
}

pub fn invalid_type(expected: &'static str, got: Option<&Value>, path: &[&str]) -> Issue {
    Issue {
        expected,
        code: "invalid_type",
        path: path.iter().map(|s| (*s).to_string()).collect(),
        message: format!(
            "Invalid input: expected {expected}, received {}",
            received(got)
        ),
    }
}

/// `value` checked against a zod type name (`string`, `object` or `record`).
pub fn check(expected: &'static str, value: Option<&Value>, path: &[&str]) -> Option<Issue> {
    let fits = matches!(
        (expected, value),
        ("string", Some(Value::String(_))) | ("object" | "record", Some(Value::Object(_)))
    );
    (!fits).then(|| invalid_type(expected, value, path))
}

/// The `JSON.stringify(issues, null, 2)` text the SDK reports as the error message.
pub fn issues_message(issues: &[Issue]) -> String {
    serde_json::to_string_pretty(issues).unwrap_or_default()
}
