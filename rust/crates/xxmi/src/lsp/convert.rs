//! Core types to LSP JSON.

use gen_lsp_types::{
    Code, Diagnostic as LspDiagnostic, DiagnosticSeverity, Message, Position as LspPosition,
    PublishDiagnosticsParams, Range as LspRange, TextDocumentContentChangeEvent, Uri,
};
use lsp_server::Notification;
use serde_json::{Value, json};
use xxmi_core::diagnostic::{Diagnostic, Position, Range, Severity};
use xxmi_core::navigation::{CompletionItem, CompletionKind, Hover, OutlineSymbol};

use super::documents::Change;

pub fn changes(events: Vec<TextDocumentContentChangeEvent>) -> Vec<Change> {
    events
        .into_iter()
        .map(|event| match event {
            TextDocumentContentChangeEvent::TextDocumentContentChangePartial(change) => Change {
                range: Some(core_range(change.range)),
                text: change.text,
            },
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(change) => {
                Change {
                    range: None,
                    text: change.text,
                }
            }
        })
        .collect()
}

pub fn core_position(position: LspPosition) -> Position {
    Position {
        line: position.line,
        character: position.character,
    }
}

fn core_range(range: LspRange) -> Range {
    Range {
        start: core_position(range.start),
        end: core_position(range.end),
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

pub fn publish(uri: &str, diagnostics: &[Diagnostic]) -> Notification {
    let params = PublishDiagnosticsParams {
        uri: Uri(uri.to_string()),
        version: None,
        diagnostics: diagnostics.iter().map(lsp_diagnostic).collect(),
    };
    Notification::new("textDocument/publishDiagnostics".to_string(), params)
}

fn lsp_diagnostic(diagnostic: &Diagnostic) -> LspDiagnostic {
    let severity = match diagnostic.severity {
        Severity::Error => DiagnosticSeverity::Error,
        Severity::Warning => DiagnosticSeverity::Warning,
        Severity::Info => DiagnosticSeverity::Information,
        Severity::Hint => DiagnosticSeverity::Hint,
    };
    let data = diagnostic
        .fix
        .as_ref()
        .and_then(|fix| serde_json::to_value(fix).ok());

    LspDiagnostic::new(
        lsp_range(diagnostic.range),
        Some(severity),
        Some(Code::String(diagnostic.id.to_string())),
        None,
        Some("xxmi".to_string()),
        Message::String(diagnostic.message.clone()),
        None,
        None,
        data,
    )
}

pub fn range_json(range: Range) -> Value {
    json!({
        "start": { "line": range.start.line, "character": range.start.character },
        "end": { "line": range.end.line, "character": range.end.character },
    })
}

pub fn hover_json(hover: Hover) -> Value {
    json!({
        "contents": { "kind": "markdown", "value": hover.contents },
        "range": range_json(hover.range),
    })
}

pub fn completion_json(item: CompletionItem) -> Value {
    let new_text = item
        .insert_text
        .clone()
        .unwrap_or_else(|| item.label.clone());
    let mut json = json!({
        "label": item.label,
        "kind": completion_kind(item.kind),
        "textEdit": { "range": range_json(item.range), "newText": new_text },
    });

    if let Some(detail) = item.detail.filter(|d| !d.is_empty()) {
        json["detail"] = json!(detail);
    }
    if let Some(documentation) = item.documentation.filter(|d| !d.is_empty()) {
        json["documentation"] = json!({ "kind": "markdown", "value": documentation });
    }
    json
}

fn completion_kind(kind: CompletionKind) -> u32 {
    match kind {
        CompletionKind::Section => 7,
        CompletionKind::Key => 10,
        CompletionKind::Command => 3,
        CompletionKind::Keyword => 14,
        CompletionKind::Value => 20,
        CompletionKind::Variable => 6,
        CompletionKind::Reference => 18,
        CompletionKind::File => 17,
    }
}

pub fn symbol_json(symbol: OutlineSymbol) -> Value {
    json!({
        "name": symbol.name,
        "kind": symbol_kind(symbol.kind),
        "range": range_json(symbol.range),
        "selectionRange": range_json(symbol.selection_range),
        "children": symbol.children.into_iter().map(symbol_json).collect::<Vec<_>>(),
    })
}

fn symbol_kind(kind: &str) -> u32 {
    match kind {
        "group" => 3,
        "CommandList" | "BuiltInCommandList" | "CustomShader" | "BuiltInCustomShader" => 12,
        "Resource" => 23,
        "Pool" => 18,
        "TextureOverride" | "ShaderOverride" | "ShaderRegex" => 24,
        "Key" | "Preset" => 20,
        "Constants" => 14,
        "Include" => 1,
        _ => 19,
    }
}
