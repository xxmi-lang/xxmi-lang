//! JSON-RPC 2.0 envelopes: message validation and the key order of results and errors.

use serde_json::{Value, json};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const SERVER_ERROR: i64 = -32000;

#[derive(Debug)]
pub struct Request {
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug)]
pub enum Message {
    Request(Request),
    Notification,
    Response,
}

/// The id of a request or response: a string or an integer, never null.
fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_i64() || id.is_u64()
}

fn valid_params(params: Option<&Value>) -> bool {
    params.is_none_or(Value::is_object)
}

/// `None` when `value` is not a JSON-RPC 2.0 request, notification or response.
pub fn classify(value: &Value) -> Option<Message> {
    let object = value.as_object()?;
    if object.get("jsonrpc")? != "2.0" {
        return None;
    }

    if let Some(method) = object.get("method") {
        let method = method.as_str()?.to_string();
        let params = object.get("params");
        if !valid_params(params) {
            return None;
        }
        return match object.get("id") {
            None => Some(Message::Notification),
            Some(id) if valid_id(id) => Some(Message::Request(Request {
                id: id.clone(),
                method,
                params: params.cloned(),
            })),
            Some(_) => None,
        };
    }

    let answered = object.get("result").is_some_and(Value::is_object)
        || object.get("error").is_some_and(Value::is_object);
    (answered && object.get("id").is_some_and(valid_id)).then_some(Message::Response)
}

/// The body of an error the transport raises before any request is dispatched.
pub fn transport_error(code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "error": { "code": code, "message": message }, "id": null })
}

pub fn success(id: &Value, result: Value) -> Value {
    json!({ "result": result, "jsonrpc": "2.0", "id": id })
}

pub fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(value: Value) -> &'static str {
        match classify(&value) {
            Some(Message::Request(_)) => "request",
            Some(Message::Notification) => "notification",
            Some(Message::Response) => "response",
            None => "invalid",
        }
    }

    #[test]
    fn classifies_messages() {
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","id":1,"method":"ping"})),
            "request"
        );
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","id":"a","method":"ping"})),
            "request"
        );
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","method":"ping"})),
            "notification"
        );
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","id":5,"result":{}})),
            "response"
        );
    }

    #[test]
    fn rejects_malformed_messages() {
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","id":null,"method":"ping"})),
            "invalid"
        );
        assert_eq!(
            kind(json!({"jsonrpc":"1.0","id":1,"method":"ping"})),
            "invalid"
        );
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","id":1.5,"method":"ping"})),
            "invalid"
        );
        assert_eq!(
            kind(json!({"jsonrpc":"2.0","id":1,"method":"ping","params":[]})),
            "invalid"
        );
        assert_eq!(kind(json!({"foo":1})), "invalid");
        assert_eq!(kind(json!("x")), "invalid");
    }

    #[test]
    fn envelopes_keep_the_sdk_key_order() {
        assert_eq!(
            success(&json!(1), json!({})).to_string(),
            r#"{"result":{},"jsonrpc":"2.0","id":1}"#
        );
        assert_eq!(
            failure(&json!(1), -32601, "x").to_string(),
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"x"}}"#
        );
    }
}
