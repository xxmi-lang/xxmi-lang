//! The files the DLL loads together with the one being linted, and the order it loads them in.

use std::cmp::Ordering;

use super::RuleContext;
use crate::model::IniFile;
use crate::workspace::FileId;
use crate::workspace::load_order::{compare_load_order, relative_to_root};

/// Files the DLL never loads (`exclude_recursive`, e.g. DISABLED folders) can only clash with
/// themselves: a disabled copy of a library isn't a duplicate of the enabled one.
/// One lint run can span several packages; the DLL only ever loads one.
pub fn loaded_files<'a>(context: &RuleContext<'a>) -> Vec<(FileId, &'a IniFile)> {
    let workspace = context.workspace;
    let excluded = workspace.is_excluded(&context.file.path, None);

    workspace
        .files()
        .iter()
        .enumerate()
        .filter(|&(id, other)| {
            let skipped =
                id != context.file_id && (excluded || workspace.is_excluded(&other.path, None));
            !skipped && workspace.package_root(other) == context.package_root
        })
        .collect()
}

/// Whether the DLL parses file `a` before file `b`.
pub fn loads_before(context: &RuleContext, a: FileId, b: FileId) -> bool {
    let files = context.workspace.files();
    let root = context.package_root;
    compare_load_order(
        relative_to_root(&files[a].path, root),
        relative_to_root(&files[b].path, root),
    ) == Ordering::Less
}
