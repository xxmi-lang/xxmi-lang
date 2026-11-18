//! `xxmi-rs lint`: the JSON document of `xxmi lint --format json` for the two ported rules.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use xxmi_core::diagnostic::{Diagnostic, Severity};
use xxmi_core::document::Document;
use xxmi_core::encoding::{Encoding, decode_text};
use xxmi_core::lint::lint;
use xxmi_core::rules::RULES;

use crate::paths::{collect_ini_files, display_path};

struct Args {
    paths: Vec<PathBuf>,
    rules: Option<Vec<String>>,
}

#[derive(Serialize)]
struct FileResult {
    path: String,
    encoding: Encoding,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Default, Serialize)]
struct Summary {
    files: usize,
    error: usize,
    warning: usize,
    info: usize,
    hint: usize,
}

/// The `version: 1` document.
#[derive(Serialize)]
struct LintOutput {
    version: u32,
    files: Vec<FileResult>,
    summary: Summary,
}

pub fn run(args: &[String]) -> Result<ExitCode, String> {
    let args = parse_args(args)?;

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for path in &args.paths {
        let found = collect_ini_files(path)
            .map_err(|error| format!("no such file or directory: {} ({error})", path.display()))?;
        for file in found.into_iter().filter(|f| seen.insert(f.clone())) {
            files.push(lint_file(&file, args.rules.as_deref())?);
        }
    }

    let summary = summarize(&files);
    let has_errors = summary.error > 0;
    let output = LintOutput {
        version: 1,
        files,
        summary,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
    );

    Ok(if has_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut paths = Vec::new();
    let mut rules = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next().map(String::as_str) {
                Some("json") => {}
                other => {
                    return Err(format!(
                        "unsupported --format {}; only json",
                        other.unwrap_or("")
                    ));
                }
            },
            "--rules" => rules = Some(parse_rules(iter.next().ok_or("--rules needs a value")?)?),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => paths.push(PathBuf::from(path)),
        }
    }

    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    Ok(Args { paths, rules })
}

fn parse_rules(list: &str) -> Result<Vec<String>, String> {
    let ids: Vec<String> = list
        .split(',')
        .map(|id| id.trim().to_uppercase())
        .filter(|id| !id.is_empty())
        .collect();

    match ids
        .iter()
        .find(|id| !RULES.iter().any(|rule| rule.id == id.as_str()))
    {
        Some(unknown) => Err(format!(
            "unknown rule {unknown} (this spike has XM001 and XM102)"
        )),
        None => Ok(ids),
    }
}

fn lint_file(path: &Path, rules: Option<&[String]>) -> Result<FileResult, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let (text, encoding) = decode_text(&bytes);
    let diagnostics = lint(&Document::new(text), rules);

    Ok(FileResult {
        path: display_path(path),
        encoding,
        diagnostics,
    })
}

fn summarize(files: &[FileResult]) -> Summary {
    let mut summary = Summary {
        files: files.len(),
        ..Summary::default()
    };
    for diagnostic in files.iter().flat_map(|f| &f.diagnostics) {
        match diagnostic.severity {
            Severity::Error => summary.error += 1,
            Severity::Warning => summary.warning += 1,
            Severity::Info => summary.info += 1,
            Severity::Hint => summary.hint += 1,
        }
    }
    summary
}
