//! Core types to `gen-lsp-types` and back.

use gen_lsp_types::{
    Code, Diagnostic as LspDiagnostic, DiagnosticSeverity, Message, Position as LspPosition,
    PublishDiagnosticsParams, Range as LspRange, TextDocumentContentChangeEvent, Uri,
};
use lsp_server::Notification;
use xxmi_core::diagnostic::{Diagnostic, Position, Range, Severity};
use xxmi_core::document::TextChange;

pub fn text_changes(events: Vec<TextDocumentContentChangeEvent>) -> Vec<TextChange> {
    events
        .into_iter()
        .map(|event| match event {
            TextDocumentContentChangeEvent::TextDocumentContentChangePartial(change) => {
                TextChange {
                    range: Some(core_range(change.range)),
                    text: change.text,
                }
            }
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(change) => {
                TextChange {
                    range: None,
                    text: change.text,
                }
            }
        })
        .collect()
}

pub fn publish(uri: &str, version: Option<i32>, diagnostics: &[Diagnostic]) -> Notification {
    let params = PublishDiagnosticsParams {
        uri: Uri(uri.to_string()),
        version,
        diagnostics: diagnostics.iter().map(lsp_diagnostic).collect(),
    };
    Notification::new("textDocument/publishDiagnostics".to_string(), params)
}

fn core_range(range: LspRange) -> Range {
    let position = |p: LspPosition| Position {
        line: p.line,
        character: p.character,
    };
    Range {
        start: position(range.start),
        end: position(range.end),
    }
}

fn lsp_range(range: Range) -> LspRange {
    let position = |p: Position| LspPosition {
        line: p.line,
        character: p.character,
    };
    LspRange {
        start: position(range.start),
        end: position(range.end),
    }
}

fn lsp_diagnostic(diagnostic: &Diagnostic) -> LspDiagnostic {
    let severity = match diagnostic.severity {
        Severity::Error => DiagnosticSeverity::Error,
        Severity::Warning => DiagnosticSeverity::Warning,
        Severity::Info => DiagnosticSeverity::Information,
        Severity::Hint => DiagnosticSeverity::Hint,
    };

    LspDiagnostic::new(
        lsp_range(diagnostic.range),
        Some(severity),
        Some(Code::String(diagnostic.id.to_string())),
        None,
        Some("xxmi".to_string()),
        Message::String(diagnostic.message.clone()),
        None,
        None,
        None,
    )
}
