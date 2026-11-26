//! Exit codes follow the LSP spec: 0 after `shutdown`, 1 without it,
//! whether the session ends with `exit` or with the client closing stdin.

mod common;

use std::process::Child;

use common::Client;
use serde_json::{Value, json};

fn start(configuration: bool) -> Client {
    let mut client = Client::start();
    let capabilities = json!({ "workspace": { "configuration": configuration } });
    client.request(
        "initialize",
        json!({ "processId": null, "rootUri": null, "capabilities": capabilities }),
    );
    client.notify("initialized", json!({}));
    client
}

fn exit_code(child: &mut Child) -> Option<i32> {
    common::wait_exit(child, "exit code").code()
}

#[test]
fn exit_right_after_the_configuration_answer_is_clean() {
    let mut client = start(true);

    let request = loop {
        let message = client.read();
        if message["method"] == json!("workspace/configuration") {
            break message;
        }
        client.handle(message);
    };
    client.handle(request);

    client.request("shutdown", Value::Null);
    client.notify("exit", Value::Null);
    assert_eq!(exit_code(&mut client.child), Some(0));
}

#[test]
fn exit_with_the_configuration_request_unanswered_is_clean() {
    let mut client = start(true);

    client.request("shutdown", Value::Null);
    client.notify("exit", Value::Null);
    assert_eq!(exit_code(&mut client.child), Some(0));
}

#[test]
fn exit_without_shutdown_is_an_error() {
    let mut client = start(false);

    client.notify("exit", Value::Null);
    assert_eq!(exit_code(&mut client.child), Some(1));
}

#[test]
fn closing_stdin_before_shutdown_is_an_error() {
    let mut client = start(false);

    drop(client.stdin);
    assert_eq!(exit_code(&mut client.child), Some(1));
}

#[test]
fn closing_stdin_after_shutdown_is_clean() {
    let mut client = start(false);

    client.request("shutdown", Value::Null);
    drop(client.stdin);
    assert_eq!(exit_code(&mut client.child), Some(0));
}

#[test]
fn requests_after_shutdown_are_refused() {
    let mut client = start(false);
    client.request("shutdown", Value::Null);

    let reply = client.call("textDocument/hover", json!({}));
    assert_eq!(reply["error"]["code"], json!(-32600));

    client.notify("exit", Value::Null);
    assert_eq!(exit_code(&mut client.child), Some(0));
}
