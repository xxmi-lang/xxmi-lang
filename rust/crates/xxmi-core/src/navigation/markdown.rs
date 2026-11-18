//! Markdown fragments shared by hover and completion documentation.

use crate::model::IniFile;
use crate::spec::Meta;
use crate::workspace::symbols::Origin;

use super::text::drop_marker;

pub fn badge(meta: &Meta) -> &'static str {
    if meta.xxmi { " · **XXMI-only**" } else { "" }
}

pub fn docs(meta: &Meta) -> String {
    let mut parts = Vec::new();
    if let Some(doc) = meta.doc.as_deref().filter(|d| !d.is_empty()) {
        parts.push(doc.to_string());
    }
    if let Some(example) = meta.example.as_deref().filter(|e| !e.is_empty()) {
        parts.push(format!("```ini\n{example}\n```"));
    }
    if let Some(deprecated) = meta.deprecated.as_deref().filter(|d| !d.is_empty()) {
        parts.push(format!("**Deprecated:** {deprecated}"));
    }
    parts.join("\n\n")
}

pub fn source(meta: &Meta) -> String {
    if meta.source == "manual" {
        return String::new();
    }
    format!("\n\n<sub>DLL: `{}`</sub>", meta.source)
}

/// The contiguous `;` comment block above `line`, or else right below it when `below` (ZZMI's
/// `; Input:` style).
pub fn doc_comment(file: &IniFile, line: u32, below: bool) -> Option<String> {
    let comment_at = |n: i64| {
        let n = u32::try_from(n).ok()?;
        file.comments.iter().find(|c| c.line == n)
    };
    let collect = |start: i64, step: i64| {
        let mut lines = Vec::new();
        let mut n = start;
        while let Some(comment) = comment_at(n) {
            lines.push(drop_marker(file.slice(comment.content)));
            n += step;
        }
        lines
    };

    let mut lines = collect(i64::from(line) - 1, -1);
    lines.reverse();
    if lines.is_empty() && below {
        lines = collect(i64::from(line) + 1, 1);
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub fn origin_text(origin: &Origin) -> String {
    match origin {
        Origin::File { .. } => String::new(),
        Origin::Snapshot { name, .. } if name == "builtin" => "Built into the DLL.".to_string(),
        Origin::Snapshot { name, path, line } => {
            format!("From the {name} library snapshot: `{path}:{line}`.")
        }
    }
}
