//! Validates tool arguments against the `inputSchema` the server advertises, with the messages of
//! the zod schemas the TypeScript server builds that schema from.

use serde_json::{Map, Value};

use crate::zod::type_name;

/// JavaScript's largest safe integer: zod's `.int()` bounds are named after it.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

fn issue(message: &str, path: &str) -> String {
    format!("{message} at {path}")
}

fn type_issue(expected: &str, value: Option<&Value>, path: &str) -> String {
    let got = value.map_or("undefined", type_name);
    issue(
        &format!("Invalid input: expected {expected}, received {got}"),
        path,
    )
}

/// zod counts a string's code points, so the bounds do too.
fn string_issues(prop: &Map<String, Value>, text: &str, path: &str) -> Vec<String> {
    let length = text.chars().count() as u64;
    let mut issues = Vec::new();
    if let Some(min) = prop.get("minLength").and_then(Value::as_u64)
        && length < min
    {
        issues.push(issue(
            &format!("Too small: expected string to have >={min} characters"),
            path,
        ));
    }
    if let Some(max) = prop.get("maxLength").and_then(Value::as_u64)
        && length > max
    {
        issues.push(issue(
            &format!("Too big: expected string to have <={max} characters"),
            path,
        ));
    }
    issues
}

/// zod keeps applying length checks to an array given where a string is expected.
fn array_issues(prop: &Map<String, Value>, items: usize, path: &str) -> Vec<String> {
    let mut issues = Vec::new();
    if let Some(min) = prop.get("minLength").and_then(Value::as_u64)
        && (items as u64) < min
    {
        issues.push(issue(
            &format!("Too small: expected array to have >={min} items"),
            path,
        ));
    }
    if let Some(max) = prop.get("maxLength").and_then(Value::as_u64)
        && (items as u64) > max
    {
        issues.push(issue(
            &format!("Too big: expected array to have <={max} items"),
            path,
        ));
    }
    issues
}

fn enum_issue(options: &[Value], value: &Value, path: &str) -> Option<String> {
    if options.contains(value) {
        return None;
    }
    let names: Vec<String> = options.iter().map(Value::to_string).collect();
    Some(issue(
        &format!("Invalid option: expected one of {}", names.join("|")),
        path,
    ))
}

fn integer_issues(prop: &Map<String, Value>, value: &Value, path: &str) -> Vec<String> {
    let Some(number) = value.as_f64().filter(|_| value.is_number()) else {
        return vec![type_issue("number", Some(value), path)];
    };

    let mut issues = Vec::new();
    if number.fract() != 0.0 {
        issues.push(issue("Invalid input: expected int, received number", path));
    }
    if let Some(min) = prop.get("minimum").and_then(Value::as_f64)
        && number < min
    {
        issues.push(issue(
            &format!("Too small: expected number to be >={min}"),
            path,
        ));
    }
    if let Some(max) = prop.get("maximum").and_then(Value::as_f64)
        && number > max
    {
        let noun = if max == MAX_SAFE_INTEGER {
            "int"
        } else {
            "number"
        };
        issues.push(issue(
            &format!("Too big: expected {noun} to be <={max}"),
            path,
        ));
    }
    issues
}

fn record_issues(value: &Value, path: &str) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return vec![type_issue("record", Some(value), path)];
    };
    object
        .iter()
        .filter(|(_, v)| !v.is_string())
        .map(|(key, v)| type_issue("string", Some(v), &format!("{path}.{key}")))
        .collect()
}

fn property_issues(prop: &Map<String, Value>, value: &Value, path: &str) -> Vec<String> {
    if let Some(options) = prop.get("enum").and_then(Value::as_array) {
        return enum_issue(options, value, path).into_iter().collect();
    }
    match prop.get("type").and_then(Value::as_str) {
        Some("string") => match value {
            Value::String(text) => string_issues(prop, text, path),
            Value::Array(items) => {
                let mut issues = vec![type_issue("string", Some(value), path)];
                issues.extend(array_issues(prop, items.len(), path));
                issues
            }
            _ => vec![type_issue("string", Some(value), path)],
        },
        Some("integer") => integer_issues(prop, value, path),
        Some("object") => record_issues(value, path),
        _ => Vec::new(),
    }
}

/// What zod calls the type a missing property was expected to have.
fn expected_type(prop: &Map<String, Value>) -> &'static str {
    match prop.get("type").and_then(Value::as_str) {
        Some("integer") => "number",
        Some("object") => "record",
        _ => "string",
    }
}

/// The arguments with defaults applied and unknown keys dropped, or every issue found, in the
/// order of the schema's properties.
pub fn validate(
    schema: &Value,
    args: &Map<String, Value>,
) -> Result<Map<String, Value>, Vec<String>> {
    let empty = Map::new();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut out = Map::new();
    let mut issues = Vec::new();
    for (key, prop) in properties {
        let Some(prop) = prop.as_object() else {
            continue;
        };
        match args.get(key) {
            Some(value) => {
                issues.extend(property_issues(prop, value, key));
                out.insert(key.clone(), value.clone());
            }
            None => match prop.get("default") {
                Some(default) => {
                    out.insert(key.clone(), default.clone());
                }
                None if required.contains(&key.as_str()) => {
                    issues.push(type_issue(expected_type(prop), None, key));
                }
                None => {}
            },
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(schema: Value, args: Value) -> Result<Map<String, Value>, Vec<String>> {
        validate(&schema, args.as_object().unwrap())
    }

    fn schema() -> Value {
        json!({"properties": {
            "name": {"type":"string","minLength":1,"maxLength":3},
            "kind": {"type":"string","enum":["a","b"]},
            "limit": {"type":"integer","minimum":1,"maximum":50},
            "game": {"type":"string","default":"zzmi"},
            "files": {"type":"object","additionalProperties":{"type":"string"}}
        }, "required": ["name"]})
    }

    #[test]
    fn applies_defaults_and_drops_unknown_keys() {
        let out = run(schema(), json!({"name":"x","extra":1})).unwrap();
        assert_eq!(Value::Object(out), json!({"name":"x","game":"zzmi"}));
    }

    #[test]
    fn reports_issues_in_schema_order_with_zod_text() {
        let issues = run(schema(), json!({"kind":"z","limit":1.5,"files":{"a":1}})).unwrap_err();
        assert_eq!(
            issues,
            [
                "Invalid input: expected string, received undefined at name",
                "Invalid option: expected one of \"a\"|\"b\" at kind",
                "Invalid input: expected int, received number at limit",
                "Invalid input: expected string, received number at files.a",
            ]
        );
    }

    #[test]
    fn checks_string_lengths_by_code_points() {
        assert!(run(schema(), json!({"name":"😀😀😀"})).is_ok());
        let issues = run(schema(), json!({"name":"😀😀😀😀"})).unwrap_err();
        assert_eq!(
            issues,
            ["Too big: expected string to have <=3 characters at name"]
        );
    }

    #[test]
    fn bounds_integers() {
        let low = run(schema(), json!({"name":"x","limit":0})).unwrap_err();
        assert_eq!(low, ["Too small: expected number to be >=1 at limit"]);
        let high = run(schema(), json!({"name":"x","limit":51})).unwrap_err();
        assert_eq!(high, ["Too big: expected number to be <=50 at limit"]);
    }
}
