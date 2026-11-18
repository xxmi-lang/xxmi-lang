//! Drives `xxmi lsp --stdio` over JSON-RPC against fixtures/lsp/package, one step per handler
//! (the same session as packages/lsp/test/protocol.test.ts).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    /// Latest diagnostics per document uri.
    diagnostics: Vec<(String, Value)>,
}

impl Client {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_xxmi"))
            .args(["lsp", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the binary starts");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 0,
            diagnostics: Vec::new(),
        }
    }

    fn write(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut length = 0;
        loop {
            let mut line = String::new();
            self.stdout
                .read_line(&mut line)
                .expect("the server answers");
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.stdout.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.write(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// The raw response: `result` or `error`.
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));

        loop {
            let message = self.read();
            if message["id"] == json!(id) && message.get("method").is_none() {
                return message;
            }
            self.handle(message);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)["result"].clone()
    }

    fn handle(&mut self, message: Value) {
        match message["method"].as_str() {
            Some("workspace/configuration") => {
                let id = message["id"].clone();
                self.write(json!({ "jsonrpc": "2.0", "id": id, "result": [{ "rules": {} }] }));
            }
            Some("textDocument/publishDiagnostics") => {
                let uri = message["params"]["uri"].as_str().unwrap().to_string();
                self.diagnostics.retain(|(known, _)| *known != uri);
                self.diagnostics
                    .push((uri, message["params"]["diagnostics"].clone()));
            }
            _ => {}
        }
    }

    /// Reads until the latest diagnostics of `uri` satisfy `accept`.
    fn diagnostics_until(&mut self, uri: &str, accept: impl Fn(&Value) -> bool) -> Value {
        loop {
            if let Some((_, found)) = self.diagnostics.iter().find(|(known, _)| known == uri)
                && accept(found)
            {
                return found.clone();
            }
            let message = self.read();
            self.handle(message);
        }
    }
}

fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/lsp/package")
        .canonicalize()
        .unwrap()
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn pos(line: u32, character: u32) -> Value {
    json!({ "line": line, "character": character })
}

fn range(from: (u32, u32), to: (u32, u32)) -> Value {
    json!({ "start": pos(from.0, from.1), "end": pos(to.0, to.1) })
}

fn insert(client: &mut Client, uri: &str, version: u32, at: (u32, u32), text: &str) {
    let changes = json!([{ "range": range(at, at), "text": text }]);
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": changes }),
    );
}

fn delete_lines(client: &mut Client, uri: &str, version: u32, from: u32, to: u32) {
    let changes = json!([{ "range": range((from, 0), (to, 0)), "text": "" }]);
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": changes }),
    );
}

fn at(uri: &str, line: u32, character: u32) -> Value {
    json!({ "textDocument": { "uri": uri }, "position": pos(line, character) })
}

fn labels(items: &Value) -> Vec<&str> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect()
}

#[test]
fn serves_a_session_over_the_fixture_package() {
    let package = package();
    let module = package.join("Mods/MyMod");
    let mod_uri = uri(&module.join("mod.ini"));
    let extra_uri = uri(&module.join("extra.ini"));
    let lib_uri = uri(&package.join("Core/Lib/lib.ini"));

    let mut client = Client::start();
    let initialized = client.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": uri(&module),
            "workspaceFolders": [{ "uri": uri(&module), "name": "MyMod" }],
            "capabilities": { "workspace": { "configuration": true } },
        }),
    );
    capabilities(&initialized);

    client.notify("initialized", json!({}));
    let text = std::fs::read_to_string(module.join("mod.ini")).unwrap();
    let item = json!({ "uri": mod_uri, "languageId": "migoto", "version": 1, "text": text });
    client.notify("textDocument/didOpen", json!({ "textDocument": item }));

    assert_eq!(client.diagnostics_until(&mod_uri, |_| true), json!([]));
    diagnostics_and_quick_fix(&mut client, &mod_uri);
    definition(&mut client, &mod_uri, &lib_uri);
    references_and_rename(&mut client, &mod_uri, &extra_uri);
    hover(&mut client, &mod_uri);
    completion(&mut client, &mod_uri);
    outline(&mut client, &mod_uri);
    save_then_edit(&mut client, &mod_uri);
    unknown_document(&mut client, &module);

    client.request("shutdown", Value::Null);
    client.notify("exit", Value::Null);
    assert!(client.child.wait().unwrap().success());
}

fn capabilities(initialized: &Value) {
    let capabilities = &initialized["capabilities"];

    assert_eq!(initialized["serverInfo"]["name"], "xxmi-lsp");
    assert_eq!(
        capabilities["textDocumentSync"],
        json!({ "openClose": true, "change": 2, "save": true })
    );
    assert_eq!(
        capabilities["renameProvider"],
        json!({ "prepareProvider": true })
    );
    assert_eq!(
        capabilities["completionProvider"]["triggerCharacters"],
        json!(["[", "$", "\\", "="])
    );
    assert_eq!(
        capabilities["codeActionProvider"]["codeActionKinds"],
        json!(["quickfix"])
    );
}

fn diagnostics_and_quick_fix(client: &mut Client, mod_uri: &str) {
    insert(client, mod_uri, 2, (9, 0), "run = CommandListLocl\n");
    let found = client.diagnostics_until(mod_uri, |d| {
        d.as_array().unwrap().iter().any(|x| x["code"] == "XM201")
    });
    let xm201 = found
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "XM201")
        .unwrap();
    assert_eq!(xm201["source"], "xxmi");
    assert_eq!(xm201["severity"], 1);
    assert_eq!(xm201["range"]["start"], pos(9, 6));

    let actions = client.request(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": mod_uri }, "range": xm201["range"], "context": { "diagnostics": [xm201] } }),
    );
    assert_eq!(actions[0]["title"], "Change to `CommandListLocal`");
    assert_eq!(actions[0]["kind"], "quickfix");
    assert_eq!(
        actions[0]["edit"]["changes"][mod_uri][0]["newText"],
        "CommandListLocal"
    );

    delete_lines(client, mod_uri, 3, 9, 10);
    assert_eq!(
        client.diagnostics_until(mod_uri, |d| d.as_array().unwrap().is_empty()),
        json!([])
    );
}

fn definition(client: &mut Client, mod_uri: &str, lib_uri: &str) {
    let location = client.request("textDocument/definition", at(mod_uri, 5, 10));

    assert_eq!(
        location,
        json!({ "uri": lib_uri, "range": range((7, 1), (7, 16)) })
    );
}

fn references_and_rename(client: &mut Client, mod_uri: &str, extra_uri: &str) {
    let locations = client.request(
        "textDocument/references",
        json!({ "textDocument": { "uri": mod_uri }, "position": pos(8, 5), "context": { "includeDeclaration": true } }),
    );
    let mut uris: Vec<&str> = locations
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["uri"].as_str().unwrap())
        .collect();
    uris.sort();
    let mut expected = vec![extra_uri, mod_uri, mod_uri];
    expected.sort();
    assert_eq!(uris, expected);

    let prepared = client.request("textDocument/prepareRename", at(mod_uri, 8, 14));
    assert_eq!(
        prepared,
        json!({ "range": range((8, 12), (8, 17)), "placeholder": "Local" })
    );

    let mut rename = at(mod_uri, 8, 14);
    rename["newName"] = json!("Shared");
    let edit = |range| json!({ "range": range, "newText": "Shared" });
    assert_eq!(
        client.request("textDocument/rename", rename)["changes"],
        json!({
            mod_uri: [edit(range((8, 12), (8, 17))), edit(range((4, 17), (4, 22)))],
            extra_uri: [edit(range((4, 17), (4, 22)))],
        })
    );

    let refused = client.call("textDocument/prepareRename", at(mod_uri, 5, 20));
    assert_eq!(refused["error"]["code"], -32600);
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("declared in the package")
    );
}

fn hover(client: &mut Client, mod_uri: &str) {
    let command = client.request("textDocument/hover", at(mod_uri, 4, 1));
    assert_eq!(command["contents"]["kind"], "markdown");
    assert!(
        command["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("**run**")
    );

    let variable = client.request("textDocument/hover", at(mod_uri, 6, 4));
    assert!(
        variable["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Global opacity used by every mod.")
    );
}

fn completion(client: &mut Client, mod_uri: &str) {
    insert(client, mod_uri, 4, (10, 0), "run = \n");
    let items = client.request("textDocument/completion", at(mod_uri, 10, 6));
    let labels = labels(&items);

    assert!(labels.contains(&"CommandListLocal"));
    assert!(labels.contains(&"CommandList\\Lib\\Draw"));
    assert!(labels.contains(&"BuiltInCommandListUnbindAllRenderTargets"));
    assert_eq!(items[0]["textEdit"]["range"]["end"], pos(10, 6));

    delete_lines(client, mod_uri, 5, 10, 11);
}

fn outline(client: &mut Client, mod_uri: &str) {
    let symbols = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": mod_uri } }),
    );
    let names: Vec<(&str, Vec<&str>)> = symbols
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap(),
                s["children"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c["name"].as_str().unwrap())
                    .collect(),
            )
        })
        .collect();

    assert_eq!(
        names,
        vec![
            ("TextureOverride", vec!["TextureOverrideBody"]),
            ("CommandList", vec!["CommandListLocal"])
        ]
    );
    assert_eq!(symbols[0]["kind"], 3);
    assert_eq!(symbols[1]["children"][0]["kind"], 12);
}

fn save_then_edit(client: &mut Client, mod_uri: &str) {
    client.notify(
        "textDocument/didSave",
        json!({ "textDocument": { "uri": mod_uri } }),
    );
    insert(client, mod_uri, 6, (9, 0), "run = CommandListAfterSave\n");

    let found =
        client.diagnostics_until(mod_uri, |d| d.to_string().contains("CommandListAfterSave"));
    assert!(
        found
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "XM201")
    );

    delete_lines(client, mod_uri, 7, 9, 10);
    assert_eq!(
        client.diagnostics_until(mod_uri, |d| d.as_array().unwrap().is_empty()),
        json!([])
    );
}

fn unknown_document(client: &mut Client, module: &Path) {
    let missing = uri(&module.join("missing.ini"));

    assert_eq!(
        client.request("textDocument/definition", at(&missing, 0, 0)),
        Value::Null
    );
    assert_eq!(
        client.request("textDocument/hover", at(&missing, 0, 0)),
        Value::Null
    );
    assert_eq!(
        client.request("textDocument/completion", at(&missing, 0, 0)),
        json!([])
    );
}
