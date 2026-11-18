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
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }

        let child = dir.join(&name);
        if file_type.is_dir() {
            walk(&child, out)?;
        } else if name.to_lowercase().ends_with(".ini") {
            out.push(child);
        }
    }
    Ok(())
}
