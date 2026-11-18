//! Hostile input over the wire: the server must stay alive and answer every request, because
//! a crash ends the editor session.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Client, at, range, uri};
use serde_json::{Value, json};

const POSITION_METHODS: [&str; 6] = [
    "textDocument/hover",
    "textDocument/definition",
    "textDocument/completion",
    "textDocument/prepareRename",
    "textDocument/references",
    "textDocument/rename",
];

/// A workspace holding one file that isn't UTF-8, removed when the test ends.
struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("xxmi-lsp-robustness-{}", std::process::id()));
        fs::create_dir_all(root.join("Mods")).unwrap();
        fs::write(
            root.join("Mods/bad.ini"),
            b"[A]\nk = \xff\xfe\x80\n[B\xc3\nrun = CommandList\xe9\n",
        )
        .unwrap();
        Self { root }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn start(root: &Path) -> Client {
    let mut client = Client::start();
    client.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": uri(root),
            "workspaceFolders": [{ "uri": uri(root), "name": "root" }],
            "capabilities": {},
        }),
    );
    client.notify("initialized", json!({}));
    client
}

fn open(client: &mut Client, uri: &str, text: &str) {
    let item = json!({ "uri": uri, "languageId": "migoto", "version": 1, "text": text });
    client.notify("textDocument/didOpen", json!({ "textDocument": item }));
}

/// The server answered, with a result or an error; anything but a reply would hang or panic here.
fn answers(client: &mut Client, method: &str, params: Value) -> Value {
    let reply = client.call(method, params);
    assert!(
        reply.get("result").is_some() || reply.get("error").is_some(),
        "{method} got no answer: {reply}"
    );
    reply
}

fn ask_everything_at(client: &mut Client, uri: &str, line: u32, character: u32) {
    for method in POSITION_METHODS {
        let mut params = at(uri, line, character);
        params["newName"] = json!("Renamed");
        params["context"] = json!({ "includeDeclaration": true });
        answers(client, method, params);
    }

    let symbols = json!({ "textDocument": { "uri": uri } });
    answers(client, "textDocument/documentSymbol", symbols);

    let context = json!({ "diagnostics": [{ "range": range((line, character), (line, character)), "message": "m", "source": "xxmi" }] });
    let params = json!({ "textDocument": { "uri": uri }, "range": range((line, character), (0, 0)), "context": context });
    answers(client, "textDocument/codeAction", params);
}

fn ask_at_the_edges(client: &mut Client, uri: &str) {
    let max = u32::MAX;
    for (line, character) in [
        (0, 0),
        (0, 1),
        (0, 9),
        (3, 0),
        (999_999, 0),
        (0, 999_999),
        (max, 0),
        (0, max),
        (max, max),
    ] {
        ask_everything_at(client, uri, line, character);
    }
}

fn change(client: &mut Client, uri: &str, version: i64, changes: Value) {
    let document = json!({ "uri": uri, "version": version });
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": document, "contentChanges": changes }),
    );
}

#[test]
fn survives_hostile_documents_and_requests() {
    let workspace = Workspace::new();
    let mut client = start(&workspace.root);

    let documents = [
        ("empty", ""),
        ("bom", "\u{feff}"),
        ("last", "[CommandListA]\nrun = CommandList\\Foo"),
        ("emoji", "[A]\nk = \u{1F600}\u{1F600}\nrun = $\\a\\b"),
        ("long", &format!("[A]\nk = {}", "x".repeat(1_000_000))),
    ];
    for (name, text) in documents {
        let uri = uri(&workspace.root.join(format!("Mods/{name}.ini")));
        open(&mut client, &uri, text);
        ask_at_the_edges(&mut client, &uri);
        client.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    let on_disk = uri(&workspace.root.join("Mods/bad.ini"));
    ask_at_the_edges(&mut client, &on_disk);
    open(&mut client, &on_disk, "[A]\nk = \u{fffd}\n");
    ask_at_the_edges(&mut client, &on_disk);

    hostile_edits(&mut client, &workspace.root);
    unknown_documents(&mut client, &workspace.root);

    client.request("shutdown", Value::Null);
    client.notify("exit", Value::Null);
    assert!(client.child.wait().unwrap().success());
}

fn hostile_edits(client: &mut Client, root: &Path) {
    let uri = uri(&root.join("Mods/edited.ini"));
    open(client, &uri, "[A]\nk = 1\n[B]\nrun = CommandListA\n");

    let max = u32::MAX;
    let edits = [
        json!([{ "range": range((99, 99), (100, 0)), "text": "tail" }]),
        json!([{ "range": range((5, 5), (0, 0)), "text": "reversed" }]),
        json!([{ "range": range((0, max), (max, max)), "text": "\u{1F600}" }]),
        json!([{ "range": range((1, 1), (1, 2)), "text": "" }, { "text": "[Z]\nrun = CommandList\\x" }]),
        json!([{ "range": range((0, 1), (0, 2)), "text": "\u{1F600}" }, { "range": range((0, 2), (0, 3)), "text": "y" }]),
    ];
    for (index, edit) in edits.into_iter().enumerate() {
        // Versions go backwards on purpose: the server doesn't track them.
        change(client, &uri, 50 - index as i64, edit);
        ask_everything_at(client, &uri, 0, 1);
    }

    // Params that don't parse are ignored, and the session carries on.
    client.notify("textDocument/didChange", json!({ "textDocument": {} }));
    client.notify("textDocument/didOpen", json!({}));
    client.notify("textDocument/didSave", Value::Null);
    ask_everything_at(client, &uri, 0, 0);
}

fn unknown_documents(client: &mut Client, root: &Path) {
    let never_opened = uri(&root.join("Mods/never-opened.ini"));
    change(
        client,
        &never_opened,
        2,
        json!([{ "range": range((0, 0), (0, 0)), "text": "x" }]),
    );
    client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": never_opened } }),
    );
    client.notify(
        "textDocument/didSave",
        json!({ "textDocument": { "uri": never_opened } }),
    );
    ask_at_the_edges(client, &never_opened);

    for odd in [
        "",
        "untitled:Untitled-1",
        "file:///",
        "file://%zz/%",
        "not a uri",
    ] {
        ask_everything_at(client, odd, 0, 0);
    }
    client.notify(
        "workspace/didChangeWatchedFiles",
        json!({ "changes": [{ "uri": never_opened, "type": 3 }, { "uri": "", "type": 2 }] }),
    );
    answers(
        client,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": never_opened } }),
    );
}
