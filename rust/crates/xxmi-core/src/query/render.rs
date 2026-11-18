//! Compact text for query results.

use super::library::SymbolInfo;
use super::spec::SpecEntry;

fn indent(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.split('\n')
        .map(|line| format!("{pad}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_spec_entry(entry: &SpecEntry) -> String {
    let section = entry
        .section
        .as_ref()
        .map_or_else(String::new, |s| format!(" (in [{s}])"));
    let xxmi = if entry.xxmi { "  [XXMI-only]" } else { "" };
    let mut lines = vec![
        format!("{} {}{section}{xxmi}", entry.kind, entry.name),
        format!("  spec source: {}", entry.source),
    ];

    if !entry.facts.is_empty() {
        lines.push(format!("  {}", entry.facts.join("; ")));
    }
    if let Some(values) = entry.values.as_ref().filter(|v| !v.is_empty()) {
        lines.push(format!("  values: {}", values.join(", ")));
    }
    if let Some(since) = &entry.since {
        lines.push(format!("  since: {since}"));
    }
    if let Some(deprecated) = &entry.deprecated {
        lines.push(format!("  deprecated: {deprecated}"));
    }
    if let Some(doc) = &entry.doc {
        lines.push(indent(doc.trim(), 2));
    }
    if let Some(example) = &entry.example {
        lines.push("  example:".to_string());
        lines.push(indent(example.trim(), 4));
    }
    lines.join("\n")
}

pub fn render_symbol(info: &SymbolInfo) -> String {
    let mut lines = vec![
        format!("{} {}", info.kind.as_str(), info.qualified_name),
        format!("declared: {}:{}", info.location.path, info.location.line),
    ];

    if let Some(doc) = &info.doc {
        lines.push("doc:".to_string());
        lines.push(indent(&doc.text, 2));
        if let Some(tags) = &doc.tags {
            for p in &tags.inputs {
                lines.push(
                    format!("input: {} {}", p.name, p.description)
                        .trim_end()
                        .to_string(),
                );
            }
            for p in &tags.outputs {
                lines.push(
                    format!("output: {} {}", p.name, p.description)
                        .trim_end()
                        .to_string(),
                );
            }
            if let Some(since) = tags.get("since") {
                lines.push(format!("since: {since}"));
            }
            if let Some(example) = &tags.example {
                lines.push("example:".to_string());
                lines.push(indent(example, 2));
            }
        }
    }
    if let Some(module) = &info.module_doc {
        lines.push(format!("file header ({}:{}):", module.path, module.line));
        lines.push(indent(&module.text, 2));
    }
    if !info.usage.is_empty() {
        lines.push(
            "mentioned in comments (usage examples; inputs are usually listed here):".to_string(),
        );
        for usage in &info.usage {
            lines.push(format!("  {}:{}", usage.path, usage.line));
            lines.push(indent(&usage.text, 4));
        }
    }
    if !info.references.is_empty() {
        lines.push("references:".to_string());
        for r in &info.references {
            lines.push(format!("  {} {} {}", r.access, r.kind.as_str(), r.text));
        }
    }
    if !info.namespace_members.is_empty() {
        lines.push("same namespace (candidate inputs/outputs):".to_string());
        for m in &info.namespace_members {
            lines.push(format!(
                "  {} {}  ({}:{})",
                m.kind.as_str(),
                m.name,
                m.path,
                m.line
            ));
        }
    }
    if let Some(source) = info.source.as_ref().filter(|s| !s.is_empty()) {
        lines.push("source:".to_string());
        lines.push(indent(source, 2));
    }
    lines.join("\n")
}
