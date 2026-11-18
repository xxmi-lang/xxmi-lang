//! The open documents and the notifications that change them.

use std::collections::HashMap;

use gen_lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
};
use lsp_server::Notification;
use serde::de::DeserializeOwned;
use xxmi_core::document::Document;

use super::BoxError;
use super::convert::{publish, text_changes};

#[derive(Default)]
pub struct Documents {
    open: HashMap<String, Document>,
}

impl Documents {
    /// Applies a document notification and returns the `publishDiagnostics` it calls for.
    pub fn handle(&mut self, notification: Notification) -> Result<Option<Notification>, BoxError> {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let params: DidOpenTextDocumentParams = parse(notification)?;
                let item = params.text_document;
                let document = Document::new(item.text);
                Ok(Some(self.store(item.uri.0, document, item.version)))
            }
            "textDocument/didChange" => {
                let params: DidChangeTextDocumentParams = parse(notification)?;
                let uri = params.text_document.text_document_identifier.uri.0;
                let version = params.text_document.version;
                let Some(mut document) = self.open.remove(&uri) else {
                    return Ok(None);
                };
                document.apply_changes(&text_changes(params.content_changes));
                Ok(Some(self.store(uri, document, version)))
            }
            "textDocument/didClose" => {
                let params: DidCloseTextDocumentParams = parse(notification)?;
                let uri = params.text_document.uri.0;
                self.open.remove(&uri);
                Ok(Some(publish(&uri, None, &[])))
            }
            _ => Ok(None),
        }
    }

    fn store(&mut self, uri: String, document: Document, version: i32) -> Notification {
        let published = publish(&uri, Some(version), &xxmi_core::lint::lint(&document, None));
        self.open.insert(uri, document);
        published
    }
}

fn parse<P: DeserializeOwned>(notification: Notification) -> Result<P, BoxError> {
    Ok(serde_json::from_value(notification.params)?)
}
