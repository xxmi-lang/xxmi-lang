//! `xxmi lsp` (also `xxmi-lsp`): the language server over stdio, see docs/03-lsp.md.
//!
//! One thread owns all the state, so the workspace is never shared: it handles a message, then
//! waits for the next one only until the nearest debounce deadline.

mod convert;
mod documents;
mod guard;
mod requests;
mod server;
mod uri;

use std::error::Error;
use std::path::PathBuf;

use crossbeam_channel::RecvTimeoutError;
use gen_lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, FileChangeType, InitializeParams,
    WorkspaceFolders,
};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use guard::guarded;
use server::Server;
use uri::uri_to_path;

type BoxError = Box<dyn Error + Send + Sync>;

/// The process exit code: 0 for an `exit` (or end of input) after `shutdown`, 1 otherwise.
pub fn run() -> Result<i32, BoxError> {
    let (connection, io_threads) = Connection::stdio();

    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let (roots, supports_configuration) = client_setup(&params);
    connection.initialize_finish(id, initialize_result())?;

    // `initialize_finish` consumes the client's `initialized` notification.
    let mut server = Server::new(&connection, roots, supports_configuration);
    server.request_settings();
    server.index_roots();
    let code = main_loop(&connection, server)?;

    // Joining ends the writer thread once the connection's sender is dropped. The reader may
    // fail to deliver a late message to the closed channel, and that is a normal way to end.
    drop(connection);
    let _ = io_threads.join();
    Ok(code)
}

/// Workspace folders, falling back to the deprecated `rootUri` some clients still send.
#[allow(deprecated)]
fn client_setup(params: &InitializeParams) -> (Vec<PathBuf>, bool) {
    let folders = match &params.workspace_folders_initialize_params.workspace_folders {
        Some(WorkspaceFolders::WorkspaceFolderList(list)) => list.as_slice(),
        _ => &[],
    };
    let mut roots: Vec<PathBuf> = folders
        .iter()
        .map(|folder| uri_to_path(&folder.uri.0))
        .collect();
    if roots.is_empty()
        && let Some(uri) = &params.root_uri
    {
        roots.push(uri_to_path(&uri.0));
    }

    let configuration = params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.configuration);
    (roots, configuration == Some(true))
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": { "openClose": true, "change": 2, "save": true },
            "definitionProvider": true,
            "referencesProvider": true,
            "renameProvider": { "prepareProvider": true },
            "hoverProvider": true,
            "completionProvider": { "triggerCharacters": ["[", "$", "\\", "="] },
            "documentSymbolProvider": true,
            "codeActionProvider": { "codeActionKinds": ["quickfix"] },
        },
        "serverInfo": { "name": "xxmi-lsp", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn main_loop(connection: &Connection, mut server: Server) -> Result<i32, BoxError> {
    let mut shut_down = false;
    let end_code = |shut_down: bool| if shut_down { 0 } else { 1 };

    loop {
        let received = match server.deadline() {
            Some(deadline) => connection
                .receiver
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())),
            None => connection
                .receiver
                .recv()
                .map_err(|_| RecvTimeoutError::Disconnected),
        };

        let message = match received {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => {
                server.run_due();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(end_code(shut_down)),
        };

        match message {
            Message::Request(request) if request.method == "shutdown" => {
                shut_down = true;
                connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, Value::Null)))?;
            }
            Message::Request(request) if shut_down => {
                let message = "the server is shutting down".to_string();
                let response =
                    Response::new_err(request.id, ErrorCode::InvalidRequest as i32, message);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Request(request) => handle_request(connection, &mut server, request)?,
            Message::Notification(notification) if notification.method == "exit" => {
                return Ok(end_code(shut_down));
            }
            Message::Notification(notification) => contain(&mut server, notification),
            Message::Response(response) => {
                server.settings_response(&response.id, response.response_result.ok())
            }
        }
    }
}

fn handle_request(
    connection: &Connection,
    server: &mut Server,
    request: Request,
) -> Result<(), BoxError> {
    let id = request.id.clone();
    let method = request.method.clone();
    let response = guarded(|| requests::handle(server, request)).unwrap_or_else(|| {
        eprintln!("xxmi-lsp: panic while handling {method}");
        let message = format!("internal error while handling {method}");
        Response::new_err(id, ErrorCode::InternalError as i32, message)
    });

    connection.sender.send(Message::Response(response))?;
    Ok(())
}

fn parse<P: DeserializeOwned>(notification: Notification) -> Result<P, BoxError> {
    Ok(serde_json::from_value(notification.params)?)
}

/// A notification with params we can't read is dropped, and one that panics costs its document
/// the cached state: neither may end the session.
fn contain(server: &mut Server, notification: Notification) {
    let method = notification.method.clone();
    let uri = document_uri(&notification);

    match guarded(|| handle_notification(server, notification)) {
        Some(Ok(())) => {}
        Some(Err(error)) => eprintln!("xxmi-lsp: ignored {method}: {error}"),
        None => {
            eprintln!("xxmi-lsp: panic while handling {method}");
            if let Some(uri) = uri {
                server.recover(&uri);
            }
        }
    }
}

fn document_uri(notification: &Notification) -> Option<String> {
    let uri = notification.params.pointer("/textDocument/uri")?;
    uri.as_str().map(str::to_string)
}

fn handle_notification(server: &mut Server, notification: Notification) -> Result<(), BoxError> {
    match notification.method.as_str() {
        "workspace/didChangeConfiguration" => server.request_settings(),
        "textDocument/didOpen" => {
            let item = parse::<DidOpenTextDocumentParams>(notification)?.text_document;
            server.did_open(item.uri.0, item.text);
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams = parse(notification)?;
            let uri = params.text_document.text_document_identifier.uri.0;
            server.did_change(&uri, convert::changes(params.content_changes));
        }
        "textDocument/didSave" => {
            let params: DidSaveTextDocumentParams = parse(notification)?;
            server.did_save(&params.text_document.uri.0);
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams = parse(notification)?;
            server.did_close(&params.text_document.uri.0);
        }
        "workspace/didChangeWatchedFiles" => {
            let params: DidChangeWatchedFilesParams = parse(notification)?;
            let changed = params
                .changes
                .into_iter()
                .map(|e| (e.uri.0, e.kind == FileChangeType::Created))
                .collect();
            server.files_changed(changed);
        }
        "xxmi/test/panic" if server.inject_panics => panic!("injected panic"),
        _ => {}
    }
    Ok(())
}
