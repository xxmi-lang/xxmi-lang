//! Lint results as text and as the versioned JSON document (schema/lint-output.v1.json).

use std::path::Path;

use serde::Serialize;

use crate::diagnostic::Diagnostic;
use crate::encoding::Encoding;

use super::{FileResult, LintResult, Summary};

/// Shows a host path to users, e.g. relative to the cwd.
pub type PathFormatter<'a> = &'a dyn Fn(&Path) -> String;

/// Files by displayed path, byte-wise: the output contract's order,
/// whatever order they were linted in.
fn files_by_path<'a>(
    result: &'a LintResult,
    path_of: PathFormatter,
) -> Vec<(String, &'a FileResult)> {
    let mut files: Vec<_> = result.files.iter().map(|f| (path_of(&f.path), f)).collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn plural(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}

/// Diagnostics as `path:line:col  severity  XM201  message`, then a summary line.
pub fn format_text(result: &LintResult, path_of: PathFormatter) -> String {
    let mut lines = Vec::new();
    for (path, file) in files_by_path(result, path_of) {
        for d in &file.diagnostics {
            let fix = if d.fix.is_some() {
                "  [fix available]"
            } else {
                ""
            };
            lines.push(format!(
                "{}:{}:{}  {}  {}  {}{fix}",
                path,
                d.range.start.line + 1,
                d.range.start.character + 1,
                d.severity.as_str(),
                d.id,
                d.message
            ));
        }
    }

    let s = &result.summary;
    lines.push(format!(
        "{}: {}, {}, {} info, {}",
        plural(s.files, "file"),
        plural(s.error, "error"),
        plural(s.warning, "warning"),
        s.info,
        plural(s.hint, "hint")
    ));
    lines.join("\n")
}

#[derive(Serialize)]
struct Document<'a> {
    version: u32,
    files: Vec<FileDocument<'a>>,
    summary: &'a Summary,
}

#[derive(Serialize)]
struct FileDocument<'a> {
    path: String,
    encoding: Encoding,
    diagnostics: &'a [Diagnostic],
}

/// The `version: 1` JSON document of `xxmi lint --format json`.
pub fn format_json(
    result: &LintResult,
    path_of: PathFormatter,
) -> Result<String, serde_json::Error> {
    let document = Document {
        version: 1,
        files: files_by_path(result, path_of)
            .into_iter()
            .map(|(path, f)| FileDocument {
                path,
                encoding: f.encoding,
                diagnostics: &f.diagnostics,
            })
            .collect(),
        summary: &result.summary,
    };
    serde_json::to_string_pretty(&document)
}
