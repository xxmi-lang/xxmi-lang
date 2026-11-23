//! Loads files into a workspace and lints them: the CLI's `xxmi lint` and `--stdin`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::encoding::Encoding;
use crate::paths::{PathResolver, resolve_path};
use crate::workspace::{FileId, Workspace};

use super::config::ConfigLoader;
use super::file::lint_file;
use super::{FileResult, LintError, LintOptions, LintResult, Summary, collect_ini_files};

/// Lints files and folders.
pub fn lint_paths(paths: &[PathBuf], options: &LintOptions) -> Result<LintResult, LintError> {
    let mut workspace = Workspace::new(options.lookup, &options.snapshots);

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
    let mut workspace = Workspace::new(options.lookup, &options.snapshots);
    let full = resolve_path(path);
    workspace.set_overlay(&full, text);

    let fallback = full.parent().unwrap_or(Path::new("")).to_path_buf();
    let id = workspace.add_target(&full, &fallback)?;
    workspace.file_mut(id).encoding = encoding;
    lint_targets(&workspace, &[id], options)
}

/// Lints in-memory files that never touch the host file system:
/// includes and path checks resolve against `files` only.
/// A d3dx.ini opens first, since it decides the package root of the rest.
pub fn lint_texts(
    files: &[(PathBuf, String)],
    options: &LintOptions,
    configs: &ConfigLoader,
) -> Result<LintResult, LintError> {
    let resolver = PathResolver::in_memory(files.iter().map(|(path, _)| path.as_path()));
    let mut workspace = Workspace::with_resolver(options.lookup, &options.snapshots, resolver);

    let mut ordered: Vec<&(PathBuf, String)> = files.iter().collect();
    ordered.sort_by_key(|(path, _)| !is_d3dx(path));
    for (path, text) in &ordered {
        workspace.set_overlay(path, text.clone());
    }

    let mut targets = Vec::new();
    for (path, text) in ordered {
        let full = resolve_path(path);
        let fallback = full.parent().unwrap_or(Path::new("")).to_path_buf();
        targets.push(workspace.open_document(&full, text.clone(), &fallback)?);
    }
    lint_targets_with(&workspace, &targets, options, configs)
}

fn is_d3dx(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("d3dx.ini"))
}

fn lint_targets(
    workspace: &Workspace,
    targets: &[FileId],
    options: &LintOptions,
) -> Result<LintResult, LintError> {
    lint_targets_with(workspace, targets, options, &ConfigLoader::default())
}

fn lint_targets_with(
    workspace: &Workspace,
    targets: &[FileId],
    options: &LintOptions,
    configs: &ConfigLoader,
) -> Result<LintResult, LintError> {
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
            diagnostics: lint_file(id, workspace, options, configs)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lints_in_memory_files_without_touching_the_disk() {
        let files = vec![(
            PathBuf::from("/xxmi-virtual/Mods/A/mod.ini"),
            "[Resource\\Foo]\nbogus_key = 1\n".to_string(),
        )];
        let result =
            lint_texts(&files, &LintOptions::bundled(), &ConfigLoader::disabled()).unwrap();
        assert_eq!(result.files.len(), 1);
        assert!(!result.files[0].diagnostics.is_empty());
    }
}
