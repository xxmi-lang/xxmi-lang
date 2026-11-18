//! The 3DMigoto folder (the package root) and the namespaces and exclusions derived from it.

use std::path::{Path, PathBuf};

use crate::glob::Glob;
use crate::paths::{PathResolver, relative_path};

/// The folder holding d3dx.ini: the "3DMigoto folder" relative paths and namespaces start from.
pub fn find_package_root(start: &Path, resolver: &PathResolver) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let has_d3dx = resolver
            .list(&dir)
            .is_some_and(|names| names.iter().any(|n| n.to_lowercase() == "d3dx.ini"));
        if has_d3dx {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Namespace the DLL gives a file: its path relative to the 3DMigoto folder, `\`-separated.
pub fn path_namespace(path: &Path, root: &Path) -> String {
    relative_path(root, path).replace(std::path::MAIN_SEPARATOR, "\\")
}

/// Used for files outside any package, where no d3dx.ini says what to skip.
pub fn default_excludes() -> Vec<Glob> {
    vec![Glob::new("DISABLED*")]
}
