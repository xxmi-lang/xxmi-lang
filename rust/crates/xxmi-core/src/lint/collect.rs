//! `.ini` files under a path, in a stable order.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths::resolve_path;

/// `.ini` files under `path` (a file is returned as is), in a stable order.
pub fn collect_ini_files(path: &Path) -> io::Result<Vec<PathBuf>> {
    let full = resolve_path(path);
    if !fs::metadata(&full)?.is_dir() {
        return Ok(vec![full]);
    }

    let mut out = Vec::new();
    walk(&full, &mut out)?;
    Ok(out)
}

/// Whether `collect_ini_files(root)` returns `path`: a `.ini` file under the resolved `root`
/// with no skipped name on the way.
pub fn is_collected(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let names: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();

    let Some(file) = names.last() else {
        return false;
    };
    is_ini(file) && !names.iter().any(|name| is_skipped(name))
}

fn is_skipped(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules"
}

fn is_ini(name: &str) -> bool {
    name.to_lowercase().ends_with(".ini")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<(String, fs::FileType)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        entries.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.file_type()?,
        ));
    }
    // JavaScript compares names by UTF-16 code unit.
    entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));

    for (name, file_type) in entries {
        if is_skipped(&name) {
            continue;
        }

        let child = dir.join(&name);
        if file_type.is_dir() {
            walk(&child, out)?;
        } else if is_ini(&name) {
            out.push(child);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_ini_files_without_hidden_or_vendored_folders() {
        let root = Path::new("/mods");
        let collected = |path: &str| is_collected(root, Path::new(path));

        assert!(collected("/mods/a/b.INI"));
        assert!(!collected("/mods/a/b.txt"));
        assert!(!collected("/mods/.git/b.ini"));
        assert!(!collected("/mods/node_modules/x/b.ini"));
        assert!(!collected("/mods/a/.hidden.ini"));
        assert!(!collected("/other/b.ini"));
        assert!(!collected("/mods"));
    }
}
