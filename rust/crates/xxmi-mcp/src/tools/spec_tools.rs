//! `spec_lookup` and `spec_list`: the merged spec, with DLL source locations when indexed.

use serde::Serialize;
use serde_json::{Map, Value, json};
use xxmi_core::query::render::{DllLocation, render_spec_entry_at};
use xxmi_core::query::spec::SpecEntry;
use xxmi_core::query::spec_all::{SpecQuery, list_spec, lookup_spec};
use xxmi_core::spec::MemberTarget;

use crate::repos::dll_repo;
use crate::sources::implementation::locate_spec_entry;
use crate::sources::permalink;
use crate::sources::tree::tree_for;
use crate::specs::ResolvedSpec;

use super::shared::ToolError;
use super::shared::{
    Args, ToolContext, ToolOutput, ToolResult, next_cursor_line, page_of, to_data, with_spec,
};

#[derive(Serialize)]
struct Located {
    path: String,
    line: usize,
    permalink: String,
    commit: String,
}

/// Where the entry's DLL symbol sits in the indexed source of the DLL the spec came from.
fn locate(ctx: &ToolContext, entry: &SpecEntry, spec: &ResolvedSpec) -> Option<Located> {
    let repo = dll_repo();
    if !ctx.catalog.is_ready(repo) || entry.source == "manual" {
        return None;
    }

    let wanted = spec.extracted.then_some(spec.dll_sha.as_str());
    let target = ctx.catalog.resolve(repo, wanted).ok()?;
    let hit = locate_spec_entry(entry, &*tree_for(&target.dir))?;
    Some(Located {
        permalink: permalink(&target, &hit.path, Some(hit.line)),
        path: hit.path,
        line: hit.line,
        commit: target.sha,
    })
}

fn member_target(name: &str) -> Option<MemberTarget> {
    match name {
        "resource" => Some(MemberTarget::Resource),
        "shader" => Some(MemberTarget::Shader),
        _ => None,
    }
}

fn entry_data(entry: &SpecEntry, located: Option<&Located>) -> Value {
    let mut data = to_data(entry);
    if let (Value::Object(map), Some(located)) = (&mut data, located) {
        map.insert("dll".into(), to_data(located));
    }
    data
}

fn render_entry(entry: &SpecEntry, located: Option<&Located>) -> String {
    let location = located.map(|l| DllLocation {
        path: &l.path,
        line: l.line,
        link: Some(&l.permalink),
    });
    render_spec_entry_at(entry, location.as_ref())
}

pub fn spec_lookup(ctx: &ToolContext, args: &Args) -> ToolResult {
    let spec = ctx.resolve_spec(args.str("dll_ref"))?;
    let name = args.required("name");
    let kind = args.text("kind");
    let query = SpecQuery {
        name,
        kind,
        section: args.text("section"),
        target: args.text("target").and_then(member_target),
    };

    let entries = lookup_spec(spec.lookup, &query);
    let located: Vec<Option<Located>> = entries.iter().map(|e| locate(ctx, e, &spec)).collect();
    let text = if entries.is_empty() {
        let of_kind = kind.map(|k| format!(" of kind {k}")).unwrap_or_default();
        format!("No spec entry named \"{name}\"{of_kind}. Use spec_list to browse names.")
    } else {
        let blocks: Vec<String> = entries
            .iter()
            .zip(&located)
            .map(|(e, l)| render_entry(e, l.as_ref()))
            .collect();
        blocks.join("\n\n")
    };

    let items: Vec<Value> = entries
        .iter()
        .zip(&located)
        .map(|(e, l)| entry_data(e, l.as_ref()))
        .collect();
    Ok(with_spec(
        ToolOutput {
            text,
            data: json!({ "entries": items }),
        },
        &spec,
    ))
}

pub fn spec_list(ctx: &ToolContext, args: &Args) -> ToolResult {
    let spec = ctx.resolve_spec(args.str("dll_ref"))?;
    let items = list_spec(spec.lookup, args.required("kind"), args.str("section"))
        .map_err(ToolError::new)?;
    let (page, next_cursor) = page_of(items, args.str("cursor"))?;

    let mut lines: Vec<String> = page
        .iter()
        .map(|i| {
            let target = i.target.map(|t| format!(" [{t}]")).unwrap_or_default();
            let form = if i.form.is_some() {
                " [value form]"
            } else {
                ""
            };
            let xxmi = if i.xxmi { " [XXMI-only]" } else { "" };
            let summary = if i.summary.is_empty() {
                String::new()
            } else {
                format!(" - {}", i.summary)
            };
            format!("{}{target}{form}{xxmi}{summary}", i.name)
        })
        .collect();
    lines.extend(next_cursor_line(next_cursor.as_deref()));

    let mut data = Map::new();
    data.insert("items".into(), to_data(&page));
    data.insert("nextCursor".into(), json!(next_cursor));
    Ok(with_spec(
        ToolOutput {
            text: lines.join("\n"),
            data: Value::Object(data),
        },
        &spec,
    ))
}
