//! `xxmi-rs lsp --stdio`: full-text diagnostics over LSP with incremental document sync.
//!
//! No debounce: every change is linted and published at once, which is what the spike measures.

mod convert;
mod documents;

use std::error::Error;

use gen_lsp_types::{
    InitializeResult, ServerCapabilities, ServerInfo, TextDocumentSync, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use lsp_server::{Connection, ErrorCode, Message, Response};

use documents::Documents;

type BoxError = Box<dyn Error + Send + Sync>;

pub fn run() -> Result<(), BoxError> {
    let (connection, io_threads) = Connection::stdio();

    let (id, _params) = connection.initialize_start()?;
    connection.initialize_finish(id, serde_json::to_value(initialize_result())?)?;
    main_loop(&connection)?;

    // Joining ends the writer thread once the connection's sender is dropped.
    drop(connection);
    io_threads.join()?;
    Ok(())
}

fn initialize_result() -> InitializeResult {
    let sync = TextDocumentSyncOptions {
        open_close: Some(true),
        change: Some(TextDocumentSyncKind::Incremental),
        ..TextDocumentSyncOptions::default()
    };

    InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSync::Options(sync)),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: "xxmi-rs".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    }
}

fn main_loop(connection: &Connection) -> Result<(), BoxError> {
    let mut documents = Documents::default();

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                let error = Response::new_err(
                    request.id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unsupported request {}", request.method),
                );
                connection.sender.send(Message::Response(error))?;
            }
            Message::Notification(notification) if notification.method == "exit" => return Ok(()),
            Message::Notification(notification) => {
                if let Some(published) = documents.handle(notification)? {
                    connection.sender.send(Message::Notification(published))?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}
