//! Loads files into a workspace and lints them: the CLI's `xxmi lint` and `--stdin`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::encoding::Encoding;
use crate::paths::resolve_path;
use crate::workspace::{FileId, Workspace};

use super::config::ConfigLoader;
use super::file::lint_file;
use super::{FileResult, LintError, LintOptions, LintResult, Summary, collect_ini_files};

/// Lints files and folders.
pub fn lint_paths(paths: &[PathBuf], options: &LintOptions) -> Result<LintResult, LintError> {
    let mut workspace = Workspace::new(options.lookup, options.snapshots);

    let mut targets = Vec::new();
    for path in paths {
        let full = resolve_path(path);
        let base = if fs::metadata(&full)?.is_dir() {
            full.clone()
        } else {
            full.parent().unwrap_or(Path::new("")).to_path_buf()
        };

        for file in collect_ini_files(&full)? {
            // Like the DLL, skip what `exclude_recursive` skips (DISABLED folders) inside a folder.
            if file != full && workspace.excluded(&file, Some(&full))? {
                continue;
            }
            targets.push(workspace.add_target(&file, &base)?);
        }
    }
    lint_targets(&workspace, &targets, options)
}

/// Lints `text` as if it were the file at `path`: the text goes into the workspace the way the
/// language server opens an editor buffer, so the namespace and the package come from `path`
/// (which doesn't have to exist). Only that file and its package's d3dx.ini includes are loaded.
pub fn lint_text(
    path: &Path,
    text: String,
    encoding: Encoding,
    options: &LintOptions,
) -> Result<LintResult, LintError> {
    let mut workspace = Workspace::new(options.lookup, options.snapshots);
    let full = resolve_path(path);
    workspace.set_overlay(&full, text);

    let fallback = full.parent().unwrap_or(Path::new("")).to_path_buf();
    let id = workspace.add_target(&full, &fallback)?;
    workspace.file_mut(id).encoding = encoding;
    lint_targets(&workspace, &[id], options)
}

fn lint_targets(
    workspace: &Workspace,
    targets: &[FileId],
    options: &LintOptions,
) -> Result<LintResult, LintError> {
    let configs = ConfigLoader::default();
    let mut seen: HashSet<&Path> = HashSet::new();
    let mut files = Vec::new();

    for &id in targets {
        let file = &workspace.files()[id];
        if !seen.insert(&file.path) {
            continue;
        }
        files.push(FileResult {
            path: file.path.clone(),
            encoding: file.encoding,
            diagnostics: lint_file(id, workspace, options, &configs)?,
        });
    }

    let summary = summarise(&files);
    Ok(LintResult { files, summary })
}

fn summarise(files: &[FileResult]) -> Summary {
    use crate::diagnostic::Severity;

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
