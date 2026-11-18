//! `library_list` and `library_symbol`: the symbols of a game package.

use serde_json::{Value, json};
use xxmi_core::query::library::{describe_snapshot_symbol, describe_symbol, list_libraries};
use xxmi_core::query::render::render_symbol_with;
use xxmi_core::workspace::snapshot::SymbolSnapshot;

use crate::sources::permalink;
use crate::specs::short;

use super::package::{PackageChoice, choose_package};
use super::shared::{
    Args, ToolContext, ToolError, ToolOutput, ToolResult, next_cursor_line, page_of, to_data,
};

fn package_label(choice: &PackageChoice) -> String {
    let at = choice
        .resolved
        .as_ref()
        .map_or("bundled snapshot", |r| r.name.as_str());
    format!(
        "{} @ {at} ({})",
        choice.snapshot().name,
        short(&choice.snapshot().source.commit)
    )
}

pub fn library_list(ctx: &ToolContext, args: &Args) -> ToolResult {
    let choice = choose_package(ctx, args.required("game"), args.str("ref"), true)?;
    let lookup = ctx.resolve_spec(None)?.lookup;
    let core = choice.loaded.as_ref().map(|l| l.core());
    let libraries = list_libraries(choice.snapshot(), lookup, core.as_ref());
    let (page, next_cursor) = page_of(libraries, args.str("cursor"))?;

    let mut head = vec![format!("package: {}", package_label(&choice))];
    head.extend(choice.note.clone());
    let blocks = page.iter().map(library_block);
    let tail = next_cursor_line(next_cursor.as_deref());
    let text: Vec<String> = head.into_iter().chain(blocks).chain(tail).collect();

    let data = json!({
        "package": to_data(&choice.snapshot().source),
        "note": choice.note,
        "libraries": to_data(&page),
        "nextCursor": next_cursor,
    });
    Ok(ToolOutput {
        text: text.join("\n\n"),
        data,
    })
}

fn library_block(lib: &xxmi_core::query::library::LibraryInfo) -> String {
    let counts: Vec<String> = lib
        .counts
        .0
        .iter()
        .map(|(kind, n)| format!("{n} {}", kind.as_str()))
        .collect();
    let files = lib
        .files
        .as_ref()
        .map(|f| format!("\n  files: {}", f.join(", ")))
        .unwrap_or_default();
    let doc = lib
        .module_doc
        .as_ref()
        .map(|d| {
            let lines: Vec<&str> = d.text.split('\n').take(12).collect();
            format!("\n  {}", lines.join("\n  "))
        })
        .unwrap_or_default();
    format!("{}  ({}){files}{doc}", lib.namespace, counts.join(", "))
}

fn suggestions(snapshot: &SymbolSnapshot, name: &str) -> Vec<String> {
    let last = name.rsplit('\\').next().unwrap_or_default().to_lowercase();
    if last.is_empty() {
        return Vec::new();
    }
    snapshot
        .sections
        .iter()
        .map(|s| s.qualified_name.as_str())
        .chain(snapshot.variables.iter().map(|v| v.key.as_str()))
        .filter(|n| n.to_lowercase().contains(&last))
        .take(8)
        .map(String::from)
        .collect()
}

pub fn library_symbol(ctx: &ToolContext, args: &Args) -> ToolResult {
    let name = args.required("qualified_name");
    let choice = choose_package(ctx, args.required("game"), args.str("ref"), true)?;
    let info = match &choice.loaded {
        Some(loaded) => describe_symbol(&loaded.core(), name),
        None => describe_snapshot_symbol(choice.snapshot(), name),
    };
    let Some(info) = info else {
        let near = suggestions(choice.snapshot(), name);
        let similar = if near.is_empty() {
            String::new()
        } else {
            format!(" Similar: {}", near.join(", "))
        };
        return Err(ToolError::new(format!(
            "No symbol \"{name}\" in {}.{similar}",
            package_label(&choice)
        )));
    };

    let package_dir = choice.resolved.as_ref().and_then(|r| r.repo.package_dir);
    let link = choice.resolved.as_ref().map(|resolved| {
        let prefix = package_dir.map(|d| format!("{d}/")).unwrap_or_default();
        permalink(
            resolved,
            &format!("{prefix}{}", info.location.path),
            Some(info.location.line),
        )
    });
    let text = render_symbol_with(&info, link.as_deref(), choice.note.as_deref());

    let data: Value = json!({
        "package": to_data(&choice.snapshot().source),
        "packageDir": package_dir,
        "note": choice.note,
        "symbol": to_data(&info),
    });
    Ok(ToolOutput { text, data })
}
