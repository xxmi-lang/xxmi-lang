//! Path handling shared with the TypeScript CLI: which files a path stands for and how they are shown.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// `.ini` files under `path` (a file is returned as is), in a stable order.
pub fn collect_ini_files(path: &Path) -> io::Result<Vec<PathBuf>> {
    let full = std::path::absolute(path)?;
    let mut files = Vec::new();

    if fs::metadata(&full)?.is_dir() {
        walk(&full, &mut files)?;
    } else {
        files.push(full);
    }
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<io::Result<_>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }

        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(&path, files)?;
        } else if name.to_lowercase().ends_with(".ini") {
            files.push(path);
        }
    }
    Ok(())
}

/// A host path shown to users: relative to the working directory when inside it, with `/`.
pub fn display_path(path: &Path) -> String {
    let shown = std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or_else(|| path.to_path_buf());

    shown
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}
