//! `xxmi lsp` (also `xxmi-lsp`): the language server over stdio, see docs/03-lsp.md.
//!
//! One thread owns all the state, so the workspace is never shared: it handles a message, then
//! waits for the next one only until the nearest debounce deadline.

mod convert;
mod documents;
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
use lsp_server::{Connection, Message, Notification, Request};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use server::Server;
use uri::uri_to_path;

type BoxError = Box<dyn Error + Send + Sync>;

pub fn run() -> Result<(), BoxError> {
    let (connection, io_threads) = Connection::stdio();

    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let (roots, supports_configuration) = client_setup(&params);
    connection.initialize_finish(id, initialize_result())?;

    // `initialize_finish` consumes the client's `initialized` notification.
    let mut server = Server::new(&connection, roots, supports_configuration);
    server.request_settings();
    server.index_roots();
    main_loop(&connection, server)?;

    // Joining ends the writer thread once the connection's sender is dropped.
    drop(connection);
    io_threads.join()?;
    Ok(())
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

fn main_loop(connection: &Connection, mut server: Server) -> Result<(), BoxError> {
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
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };

        match message {
            Message::Request(request) => {
                if handle_request(connection, &mut server, request)? {
                    return Ok(());
                }
            }
            Message::Notification(notification) if notification.method == "exit" => return Ok(()),
            Message::Notification(notification) => handle_notification(&mut server, notification)?,
            Message::Response(response) => {
                server.settings_response(&response.id, response.response_result.ok())
            }
        }
    }
}

/// Returns true once the client asked to shut down.
fn handle_request(
    connection: &Connection,
    server: &mut Server,
    request: Request,
) -> Result<bool, BoxError> {
    if connection.handle_shutdown(&request)? {
        return Ok(true);
    }
    connection
        .sender
        .send(Message::Response(requests::handle(server, request)))?;
    Ok(false)
}

fn parse<P: DeserializeOwned>(notification: Notification) -> Result<P, BoxError> {
    Ok(serde_json::from_value(notification.params)?)
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
        _ => {}
    }
    Ok(())
}
