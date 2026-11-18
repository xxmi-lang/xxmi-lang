//! Request handlers: each runs against the current model and answers with the JSON the
//! TypeScript server sends.

use gen_lsp_types::{
    CodeActionParams, DocumentSymbolParams, ReferenceParams, RenameParams,
    TextDocumentPositionParams,
};
use lsp_server::{ErrorCode, Request, Response};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use xxmi_core::diagnostic::Position;
use xxmi_core::navigation::{
    Location, completions, definition, hover, outline, references, rename, target_at,
};
use xxmi_core::workspace::FileId;

use super::convert::{completion_json, core_position, hover_json, range_json, symbol_json};
use super::server::Server;

/// A JSON result, or an error code and message for the client.
type Reply = Result<Value, (i32, String)>;

pub fn handle(server: &mut Server, request: Request) -> Response {
    let id = request.id.clone();
    let reply = match request.method.as_str() {
        "textDocument/definition" => with_position(server, request, Value::Null, definition_at),
        "textDocument/references" => references_at(server, request),
        "textDocument/prepareRename" => with_position(server, request, Value::Null, prepare_rename),
        "textDocument/rename" => rename_at(server, request),
        "textDocument/hover" => with_position(server, request, Value::Null, hover_at),
        "textDocument/completion" => with_position(server, request, json!([]), completion_at),
        "textDocument/documentSymbol" => document_symbols(server, request),
        "textDocument/codeAction" => code_actions(server, request),
        "xxmi/test/panic" if server.inject_panics => panic!("injected panic"),
        method => Err((
            ErrorCode::MethodNotFound as i32,
            format!("Unhandled method {method}"),
        )),
    };

    match reply {
        Ok(result) => Response::new_ok(id, result),
        Err((code, message)) => Response::new_err(id, code, message),
    }
}

fn params<P: DeserializeOwned>(request: Request) -> Result<P, (i32, String)> {
    serde_json::from_value(request.params)
        .map_err(|error| (ErrorCode::InvalidParams as i32, error.to_string()))
}

fn invalid_request(message: String) -> (i32, String) {
    (ErrorCode::InvalidRequest as i32, message)
}

/// `missing` is the answer for a file that isn't loaded.
fn with_position(
    server: &mut Server,
    request: Request,
    missing: Value,
    handler: fn(&mut Server, FileId, Position) -> Reply,
) -> Reply {
    let params: TextDocumentPositionParams = params(request)?;
    let uri = params.text_document.uri.0;
    let position = core_position(params.position);

    match server.file_id(&uri) {
        Some(id) => handler(server, id, position),
        None => Ok(missing),
    }
}

fn location_json(server: &Server, location: Location) -> Value {
    json!({ "uri": server.to_uri(&location.path), "range": range_json(location.range) })
}

fn definition_at(server: &mut Server, id: FileId, position: Position) -> Reply {
    let target = target_at(&server.workspace, id, position);
    let location = target.and_then(|t| definition(&server.workspace, &t));
    Ok(location.map_or(Value::Null, |l| location_json(server, l)))
}

fn references_at(server: &mut Server, request: Request) -> Reply {
    let params: ReferenceParams = params(request)?;
    let position = &params.text_document_position_params;
    let uri = &position.text_document.uri.0;

    let Some(id) = server.file_id(uri) else {
        return Ok(json!([]));
    };
    let Some(target) = target_at(&server.workspace, id, core_position(position.position)) else {
        return Ok(json!([]));
    };

    let found = references(
        &server.workspace,
        &target,
        params.context.include_declaration,
    );
    Ok(found
        .into_iter()
        .map(|l| location_json(server, l))
        .collect())
}

fn prepare_rename(server: &mut Server, id: FileId, position: Position) -> Reply {
    let outcome = rename(&server.workspace, id, position, None).map_err(invalid_request)?;
    Ok(json!({ "range": range_json(outcome.range), "placeholder": outcome.placeholder }))
}

fn rename_at(server: &mut Server, request: Request) -> Reply {
    let params: RenameParams = params(request)?;
    let uri = &params.text_document_position_params.text_document.uri.0;
    let position = core_position(params.text_document_position_params.position);

    let Some(id) = server.file_id(uri) else {
        return Ok(Value::Null);
    };
    let outcome =
        rename(&server.workspace, id, position, Some(&params.new_name)).map_err(invalid_request)?;

    let mut changes = serde_json::Map::new();
    for (path, edits) in outcome.edits {
        changes.insert(
            server.to_uri(&path),
            serde_json::to_value(edits).unwrap_or(Value::Null),
        );
    }
    Ok(json!({ "changes": changes }))
}

fn hover_at(server: &mut Server, id: FileId, position: Position) -> Reply {
    Ok(hover(&server.workspace, id, position).map_or(Value::Null, hover_json))
}

fn completion_at(server: &mut Server, id: FileId, position: Position) -> Reply {
    let items = completions(&server.workspace, id, position);
    Ok(items.into_iter().map(completion_json).collect())
}

fn document_symbols(server: &mut Server, request: Request) -> Reply {
    let params: DocumentSymbolParams = params(request)?;
    let Some(id) = server.file_id(&params.text_document.uri.0) else {
        return Ok(json!([]));
    };
    let symbols = outline(&server.workspace.files()[id]);
    Ok(symbols.into_iter().map(symbol_json).collect())
}

fn code_actions(_: &mut Server, request: Request) -> Reply {
    let params: CodeActionParams = params(request)?;
    let uri = params.text_document.uri.0;

    let mut actions = Vec::new();
    for diagnostic in params.context.diagnostics {
        let fix = diagnostic.data.as_ref();
        let edits = fix.and_then(|f| f.get("edits")).cloned();
        let title = fix.and_then(|f| f.get("title"));
        if diagnostic.source.as_deref() != Some("xxmi") || edits.is_none() {
            continue;
        }
        actions.push(json!({
            "title": title,
            "kind": "quickfix",
            "diagnostics": [diagnostic],
            "isPreferred": true,
            "edit": { "changes": { uri.clone(): edits } },
        }));
    }
    Ok(Value::Array(actions))
}
