//! A JSON-RPC client for `xxmi lsp --stdio`, shared by the LSP integration tests.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// A server that stays silent this long is hung: a CI job that waited forever left no log.
const READ_DEADLINE: Duration = Duration::from_secs(30);
const EXIT_DEADLINE: Duration = Duration::from_secs(10);

pub struct Client {
    pub child: Child,
    pub stdin: ChildStdin,
    /// Messages parsed by a reader thread, so `read` can give up instead of blocking on the pipe.
    pub messages: Receiver<Value>,
    /// The latest message sent, to name what a timed-out read was waiting for.
    pub last_sent: String,
    pub next_id: i64,
    /// Latest diagnostics per document uri.
    pub diagnostics: Vec<(String, Value)>,
}

impl Client {
    pub fn start() -> Self {
        Self::start_with_env(&[])
    }

    pub fn start_with_env(env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_xxmi"))
            .args(["lsp", "--stdio"])
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the binary starts");
        let stdin = child.stdin.take().unwrap();
        let messages = spawn_reader(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            messages,
            last_sent: String::from("(nothing)"),
            next_id: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn write(&mut self, message: Value) {
        let body = message.to_string();
        self.last_sent = message["method"]
            .as_str()
            .unwrap_or("a response")
            .to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    pub fn read(&mut self) -> Value {
        match self.messages.recv_timeout(READ_DEADLINE) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                panic!(
                    "{}: no message from the server within {READ_DEADLINE:?}; the last message sent was `{}`",
                    test_name(),
                    self.last_sent
                );
            }
            Err(RecvTimeoutError::Disconnected) => panic!(
                "{}: the server closed stdout; the last message sent was `{}`",
                test_name(),
                self.last_sent
            ),
        }
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

fn test_name() -> String {
    std::thread::current().name().unwrap_or("test").to_string()
}

fn spawn_reader(stdout: ChildStdout) -> Receiver<Value> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        while let Some(message) = read_message(&mut stdout) {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    receiver
}

/// `None` once the pipe closes.
fn read_message(stdout: &mut BufReader<ChildStdout>) -> Option<Value> {
    let mut length = 0;
    loop {
        let mut line = String::new();
        if stdout.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            length = value.parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    stdout.read_exact(&mut body).ok()?;
    Some(serde_json::from_slice(&body).unwrap())
}

/// Waits for the server to exit; `step` names what the test was waiting for.
pub fn wait_exit(child: &mut Child, step: &str) -> ExitStatus {
    let deadline = Instant::now() + EXIT_DEADLINE;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!(
                "{}: the server did not exit within {EXIT_DEADLINE:?} ({step})",
                test_name()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/lsp/package")
        .canonicalize()
        .unwrap()
}

/// A `file:` URI; Windows paths need forward slashes, a leading `/` and no `\\?\` marker (which
/// `canonicalize` adds).
pub fn uri(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let slash = if text.starts_with('/') { "" } else { "/" };
    format!("file://{slash}{text}")
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
