//! A JSON-RPC client for `xxmi lsp --stdio`, shared by the LSP integration tests.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

pub struct Client {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: BufReader<ChildStdout>,
    pub next_id: i64,
    /// Latest diagnostics per document uri.
    pub diagnostics: Vec<(String, Value)>,
}

impl Client {
    pub fn start() -> Self {
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

    pub fn write(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    pub fn read(&mut self) -> Value {
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

    pub fn notify(&mut self, method: &str, params: Value) {
        self.write(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// The raw response: `result` or `error`.
    pub fn call(&mut self, method: &str, params: Value) -> Value {
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

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)["result"].clone()
    }

    pub fn handle(&mut self, message: Value) {
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
    pub fn diagnostics_until(&mut self, uri: &str, accept: impl Fn(&Value) -> bool) -> Value {
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

pub fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/lsp/package")
        .canonicalize()
        .unwrap()
}

pub fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

pub fn pos(line: u32, character: u32) -> Value {
    json!({ "line": line, "character": character })
}

pub fn range(from: (u32, u32), to: (u32, u32)) -> Value {
    json!({ "start": pos(from.0, from.1), "end": pos(to.0, to.1) })
}

pub fn insert(client: &mut Client, uri: &str, version: u32, at: (u32, u32), text: &str) {
    let changes = json!([{ "range": range(at, at), "text": text }]);
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": changes }),
    );
}

pub fn delete_lines(client: &mut Client, uri: &str, version: u32, from: u32, to: u32) {
    let changes = json!([{ "range": range((from, 0), (to, 0)), "text": "" }]);
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": changes }),
    );
}

pub fn at(uri: &str, line: u32, character: u32) -> Value {
    json!({ "textDocument": { "uri": uri }, "position": pos(line, character) })
}
