//! `lint_ini`: lints submitted text against the indexed package, never touching the server's disk.

use std::collections::{HashMap, HashSet};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::slice;
use std::time::Instant;

use serde_json::{Map, Value, json};
use xxmi_core::lint::format::{format_json, format_text};
use xxmi_core::lint::{ConfigLoader, LintOptions, LintResult, lint_texts};

use super::package::choose_package;
use super::shared::{Args, ToolContext, ToolError, ToolOutput, ToolResult, with_spec};

/// Where the submitted files live for the linter; nothing at or below it is ever read.
const VIRTUAL_ROOT: &str = "/xxmi-virtual";
const MAX_PATH_CHARS: usize = 260;
const MAX_REPORTED_DIAGNOSTICS: usize = 500;

fn invalid_path(raw: &str) -> ToolError {
    let shown: String = raw.chars().take(80).collect();
    ToolError::new(format!(
        "invalid path \"{shown}\": use a relative path like Mods/MyMod/mod.ini"
    ))
}

fn has_drive_prefix(path: &str) -> bool {
    let mut chars = path.chars();
    matches!((chars.next(), chars.next()), (Some(letter), Some(':')) if letter.is_ascii_alphabetic())
}

/// Normalizes a caller-supplied path to a clean relative `a/b.ini`, or fails.
pub fn clean_virtual_path(raw: &str) -> Result<String, ToolError> {
    let mut path = raw.replace('\\', "/");
    while let Some(rest) = path.strip_prefix("./") {
        path = rest.to_string();
    }

    let bad = path.is_empty()
        || path.encode_utf16().count() > MAX_PATH_CHARS
        || path.contains('\0')
        || path.starts_with('/')
        || has_drive_prefix(&path)
        || path
            .split('/')
            .any(|s| s.is_empty() || s == ".." || s == ".");
    if bad {
        return Err(invalid_path(raw));
    }
    if !path.to_lowercase().ends_with(".ini") {
        let shown: String = raw.chars().take(80).collect();
        return Err(ToolError::new(format!("\"{shown}\" is not an .ini file")));
    }
    Ok(path)
}

pub fn check_lint_input(
    ctx: &ToolContext,
    files: &Map<String, Value>,
) -> Result<Vec<(String, String)>, ToolError> {
    if files.is_empty() {
        return Err(ToolError::new("files is empty"));
    }
    if files.len() > ctx.config.max_lint_files {
        return Err(ToolError::new(format!(
            "too many files: {} (limit {})",
            files.len(),
            ctx.config.max_lint_files
        )));
    }

    let mut seen = HashSet::new();
    let mut bytes = 0;
    let mut checked = Vec::new();
    for (raw, text) in files {
        let path = clean_virtual_path(raw)?;
        if !seen.insert(path.to_lowercase()) {
            return Err(ToolError::new(format!("duplicate path \"{path}\"")));
        }
        let text = text.as_str().unwrap_or_default().to_string();
        bytes += text.len();
        checked.push((path, text));
    }
    if bytes > ctx.config.max_lint_bytes {
        return Err(ToolError::new(format!(
            "input too large: {bytes} bytes (limit {})",
            ctx.config.max_lint_bytes
        )));
    }
    Ok(checked)
}

/// Trims diagnostics in place, oldest files first; the summary keeps counting every one.
fn cap_diagnostics(result: &mut LintResult) -> usize {
    let mut budget = MAX_REPORTED_DIAGNOSTICS;
    let mut dropped = 0;
    for file in &mut result.files {
        let kept = file.diagnostics.len().min(budget);
        budget -= kept;
        dropped += file.diagnostics.len() - kept;
        file.diagnostics.truncate(kept);
    }
    dropped
}

fn relative_to_root(path: &Path) -> String {
    let relative = path.strip_prefix(VIRTUAL_ROOT).unwrap_or(path);
    relative.to_string_lossy().replace(MAIN_SEPARATOR, "/")
}

pub fn lint_ini(ctx: &ToolContext, args: &Args) -> ToolResult {
    let Some(Value::Object(submitted)) = args.0.get("files") else {
        return Err(ToolError::new("files is empty"));
    };
    let files = check_lint_input(ctx, submitted)?;
    let spec = ctx.resolve_spec(args.str("dll_ref"))?;
    let choice = choose_package(ctx, args.required("game"), args.str("package_ref"), false)?;

    let root = PathBuf::from(VIRTUAL_ROOT);
    let virtual_files: Vec<(PathBuf, String)> = files
        .iter()
        .map(|(path, text)| (root.join(path), text.clone()))
        .collect();
    let options = LintOptions {
        rules: None,
        overrides: HashMap::new(),
        rule_prefixes: None,
        lookup: spec.lookup,
        snapshots: slice::from_ref(choice.snapshot),
    };

    let started = Instant::now();
    let mut result = lint_texts(&virtual_files, &options, &ConfigLoader::disabled())
        .map_err(|e| ToolError::internal(e.to_string()))?;
    // Sizes and counts only: the submitted text must never reach the logs.
    ctx.logger.info(
        "lint_ini",
        json!({
            "files": files.len(),
            "bytes": files.iter().map(|(_, text)| text.len()).sum::<usize>(),
            "ms": started.elapsed().as_millis() as u64,
            "errors": result.summary.error,
            "warnings": result.summary.warning,
        }),
    );

    let dropped = cap_diagnostics(&mut result);
    let path_of = |path: &Path| relative_to_root(path);
    let mut doc: Value = serde_json::from_str(
        &format_json(&result, &path_of).map_err(|e| ToolError::internal(e.to_string()))?,
    )
    .map_err(|e| ToolError::internal(e.to_string()))?;
    if let Value::Object(map) = &mut doc {
        map.insert(
            "package".into(),
            serde_json::to_value(&choice.snapshot.source).unwrap_or(Value::Null),
        );
        if dropped > 0 {
            map.insert("truncated".into(), dropped.into());
        }
    }

    let mut lines = vec![format_text(&result, &path_of)];
    if dropped > 0 {
        lines.push(format!("({dropped} more diagnostics not shown)"));
    }
    lines.extend(choice.note);
    Ok(with_spec(
        ToolOutput {
            text: lines.join("\n"),
            data: doc,
        },
        &spec,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_relative_paths() {
        assert_eq!(clean_virtual_path(".\\Mods\\a.INI").unwrap(), "Mods/a.INI");
    }

    #[test]
    fn rejects_escapes_and_other_extensions() {
        for bad in [
            "",
            "/etc/x.ini",
            "C:/x.ini",
            "../x.ini",
            "a//b.ini",
            "a/./b.ini",
            "a.txt",
        ] {
            assert!(clean_virtual_path(bad).is_err(), "{bad}");
        }
    }
}
