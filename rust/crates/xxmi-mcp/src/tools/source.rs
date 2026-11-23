//! `find_implementation`, `search_source` and `read_source`: the indexed repos' source.

use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::catalog::ResolvedRef;
use crate::repos::{TrackedRepo, dll_repo};
use crate::sources::excerpt::read_excerpt;
use crate::sources::implementation::{ImplementationHit, find_implementation};
use crate::sources::permalink;
use crate::sources::search::{SearchMatch, SearchOptions, search_source};
use crate::sources::tree::tree_for;
use crate::specs::short;

use xxmi_core::query::spec::SpecEntry;

use super::blocking::checkpoint;
use super::shared::{Args, ToolContext, ToolOutput, ToolResult, spread, to_data, with_spec};

fn entry_line(entry: &SpecEntry) -> String {
    let xxmi = if entry.xxmi { " [XXMI-only]" } else { "" };
    format!(
        "spec: {} {} -> {}{xxmi}",
        entry.kind, entry.name, entry.source
    )
}

fn hit_block(hit: &ImplementationHit) -> String {
    let function = hit
        .in_function
        .as_ref()
        .map(|f| format!(" in {f}"))
        .unwrap_or_default();
    let link = hit.permalink.as_deref().unwrap_or_default();
    format!(
        "{}:{}  {}{function}\n    {}\n    {link}",
        hit.path,
        hit.line,
        hit.role.as_str(),
        hit.snippet
    )
}

fn implementation_text(
    target: &ResolvedRef,
    feature: &str,
    entries: &[SpecEntry],
    hits: &[ImplementationHit],
) -> String {
    let mut lines = vec![format!(
        "DLL source: {} @ {} ({})",
        dll_repo().full_name(),
        target.name,
        short(&target.sha)
    )];
    lines.extend(entries.iter().map(entry_line));
    if hits.is_empty() {
        lines.push(format!("No matches for \"{feature}\" in the DLL source."));
    }
    lines.extend(hits.iter().map(hit_block));
    lines.join("\n")
}

pub fn find_implementation_tool(ctx: &ToolContext, args: &Args) -> ToolResult {
    let target = ctx.catalog.resolve(dll_repo(), args.str("ref"))?;
    // The spec of the DLL version being searched, so its symbols exist in that source.
    let spec = ctx
        .specs
        .for_version(&target, args.str("ref").unwrap_or("head"));
    let feature = args.required("feature");
    let result = find_implementation(
        spec.lookup,
        feature,
        &*tree_for(&target.dir),
        args.count("limit").unwrap_or(20),
    );
    checkpoint()?;

    let mut hits = result.hits;
    for hit in &mut hits {
        hit.permalink = Some(permalink(&target, &hit.path, Some(hit.line)));
    }

    let data = json!({
        "dll": { "ref": target.name, "sha": target.sha },
        "entries": to_data(&result.entries),
        "hits": to_data(&hits),
    });
    let text = implementation_text(&target, feature, &result.entries, &hits);
    Ok(with_spec(ToolOutput { text, data }, &spec))
}

/// The repo, ref and commit a source result is about.
fn source_head(repo: &TrackedRepo, target: &ResolvedRef) -> Map<String, Value> {
    let mut head = Map::new();
    head.insert("repo".into(), repo.full_name().into());
    head.insert("ref".into(), target.name.clone().into());
    head.insert("sha".into(), target.sha.clone().into());
    head
}

fn context_lines(found: &SearchMatch, after: bool) -> impl Iterator<Item = String> + '_ {
    found
        .context
        .iter()
        .filter(move |c| (c.line > found.line) == after && c.line != found.line)
        .map(|c| format!("{}- {}", c.line, c.text))
}

fn match_block(target: &ResolvedRef, found: &SearchMatch) -> String {
    let link = permalink(target, &found.path, Some(found.line as usize));
    let body: Vec<String> = context_lines(found, false)
        .chain(std::iter::once(format!("{}: {}", found.line, found.text)))
        .chain(context_lines(found, true))
        .collect();
    format!("{}:{}  {link}\n{}", found.path, found.line, body.join("\n"))
}

pub async fn search_source_tool(ctx: &ToolContext, args: &Args) -> ToolResult {
    let repo = ctx.catalog.repo_by_name(args.required("repo"))?;
    let target = ctx.catalog.resolve(repo, args.str("ref"))?;
    let options = SearchOptions {
        path_glob: args.str("path_glob"),
        max_results: args.count("max_results").unwrap_or(20),
        timeout: Duration::from_millis(ctx.config.search_timeout_ms),
    };
    let result = search_source(&target.dir, args.required("query"), &options).await?;

    let mut sections = vec![format!(
        "{} @ {} ({})",
        repo.full_name(),
        target.name,
        short(&target.sha)
    )];
    sections.extend(result.matches.iter().map(|m| match_block(&target, m)));
    if result.truncated {
        sections.push("(more matches exist; narrow the query or path_glob)".into());
    }
    if result.timed_out {
        sections.push(format!(
            "(search stopped after {} ms; results are partial)",
            ctx.config.search_timeout_ms
        ));
    }
    if result.matches.is_empty() {
        sections.push("No matches.".into());
    }

    Ok(ToolOutput {
        text: sections.join("\n\n"),
        data: spread(source_head(repo, &target), &result),
    })
}

pub fn read_source_tool(ctx: &ToolContext, args: &Args) -> ToolResult {
    let repo = ctx.catalog.repo_by_name(args.required("repo"))?;
    let target = ctx.catalog.resolve(repo, args.str("ref"))?;
    let start = args.count("start_line").unwrap_or(1);
    let end = args.count("end_line").unwrap_or(1);
    let excerpt = read_excerpt(&target.dir, args.required("path"), start, end)?;

    let link = permalink(&target, &excerpt.path, Some(excerpt.start_line));
    let text = format!(
        "{} lines {}-{} of {}  {link}\n{}",
        excerpt.path, excerpt.start_line, excerpt.end_line, excerpt.total_lines, excerpt.text
    );

    let mut head = source_head(repo, &target);
    head.insert("permalink".into(), Value::String(link));
    Ok(ToolOutput {
        text,
        data: spread(head, &excerpt),
    })
}
