//! A panic in a handler must not end the session: a request gets an error reply and a
//! notification costs its document the cached state, which the next message rebuilds.

mod common;

use common::{Client, at, insert, uri};
use serde_json::{Value, json};

const INTERNAL_ERROR: i64 = -32603;

fn start() -> Client {
    let mut client = Client::start_with_env(&[("XXMI_LSP_INJECT_PANIC", "1")]);
    client.request(
        "initialize",
        json!({ "processId": null, "rootUri": null, "capabilities": {} }),
    );
    client.notify("initialized", json!({}));
    client
}

fn symbol_count(client: &mut Client, uri: &str) -> usize {
    let reply = client.call(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    reply["result"].as_array().expect("symbols answer").len()
}

#[test]
fn a_panicking_request_gets_an_error_and_the_session_carries_on() {
    let mut client = start();
    let uri = uri(&std::env::temp_dir().join("panic-request.ini"));
    let item = json!({ "uri": uri, "languageId": "migoto", "version": 1, "text": "[ResourceA]\n" });
    client.notify("textDocument/didOpen", json!({ "textDocument": item }));

    let reply = client.call("xxmi/test/panic", at(&uri, 0, 0));
    assert_eq!(reply["error"]["code"], json!(INTERNAL_ERROR));
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("xxmi/test/panic")
    );

    assert_eq!(symbol_count(&mut client, &uri), 1);
    client.request("shutdown", Value::Null);
    client.notify("exit", Value::Null);
    assert!(common::wait_exit(&mut client.child, "exit after shutdown").success());
}

#[test]
fn a_panicking_notification_resyncs_the_document() {
    let mut client = start();
    let uri = uri(&std::env::temp_dir().join("panic-notification.ini"));
    let item = json!({ "uri": uri, "languageId": "migoto", "version": 1, "text": "[ResourceA]\n" });
    client.notify("textDocument/didOpen", json!({ "textDocument": item }));
    client.diagnostics_until(&uri, |_| true);

    client.notify("xxmi/test/panic", json!({ "textDocument": { "uri": uri } }));
    assert_eq!(symbol_count(&mut client, &uri), 1);

    insert(&mut client, &uri, 2, (0, 0), "[Constants]\n");
    assert_eq!(symbol_count(&mut client, &uri), 2);
    client.request("shutdown", Value::Null);
    client.notify("exit", Value::Null);
    assert!(common::wait_exit(&mut client.child, "exit after shutdown").success());
}
