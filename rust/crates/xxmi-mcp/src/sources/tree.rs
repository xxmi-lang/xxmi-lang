//! A checkout as a read-only source tree,
//! shared per directory because checkouts never change once published.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use xxmi_core::encoding::decode_text;
use xxmi_core::query::utf16_cmp;

/// What the DLL scans need of a checkout; paths are `/`-separated and relative to its root.
pub trait SourceTree {
    fn files(&self) -> &[String];
    fn read(&self, path: &str) -> Option<String>;
}

/// `relative` inside `root` as a normalized `/`-joined path, or `None` when it is absolute,
/// has a NUL, or escapes the root.
pub fn safe_relative(relative: &str) -> Option<String> {
    let drive = relative.as_bytes().get(1) == Some(&b':')
        && relative
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    if relative.is_empty() || relative.contains('\0') || relative.starts_with('/') || drive {
        return None;
    }

    let slashed = relative.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in slashed.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    let first = parts.first()?;
    (!first.starts_with("..")).then(|| parts.join("/"))
}

pub fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    safe_relative(relative).map(|inside| root.join(inside))
}

pub struct CheckoutTree {
    root: PathBuf,
    list: OnceLock<Vec<String>>,
    texts: Mutex<HashMap<String, Option<String>>>,
}

/// The DLL scan touches ~100 files and a few trees are alive at a time.
const MAX_CACHED_TEXTS: usize = 400;

impl CheckoutTree {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            list: OnceLock::new(),
            texts: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, path: &str) -> Option<Option<String>> {
        let texts = self.texts.lock().unwrap_or_else(|e| e.into_inner());
        texts.get(path).cloned()
    }
}

impl SourceTree for CheckoutTree {
    fn files(&self) -> &[String] {
        self.list.get_or_init(|| {
            let mut out = Vec::new();
            walk(&self.root, &self.root, &mut out);
            out.sort_by(|a, b| utf16_cmp(a, b));
            out
        })
    }

    fn read(&self, path: &str) -> Option<String> {
        if let Some(cached) = self.cached(path) {
            return cached;
        }

        let text = safe_join(&self.root, path)
            .and_then(|full| fs::read(full).ok())
            .map(|bytes| decode_text(&bytes).0);

        // Two readers may load the same file; the contents are equal, so either insert may win.
        let mut texts = self.texts.lock().unwrap_or_else(|e| e.into_inner());
        if texts.len() < MAX_CACHED_TEXTS {
            texts.insert(path.to_string(), text.clone());
        }
        text
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            walk(root, &path, out);
        } else if kind.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            let parts: Vec<_> = relative.iter().map(|p| p.to_string_lossy()).collect();
            out.push(parts.join("/"));
        }
    }
}

const MAX_TREES: usize = 6;

/// Oldest first.
static TREES: Mutex<Vec<(PathBuf, Arc<CheckoutTree>)>> = Mutex::new(Vec::new());

pub fn tree_for(dir: &Path) -> Arc<CheckoutTree> {
    let mut trees = TREES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, tree)) = trees.iter().find(|(known, _)| known == dir) {
        return tree.clone();
    }

    let tree = Arc::new(CheckoutTree::new(dir.to_path_buf()));
    trees.push((dir.to_path_buf(), tree.clone()));
    if trees.len() > MAX_TREES {
        trees.remove(0);
    }
    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_paths_inside_the_root() {
        assert_eq!(safe_relative("a/./b//c.cpp").as_deref(), Some("a/b/c.cpp"));
        assert_eq!(safe_relative("a/../b").as_deref(), Some("b"));
        assert_eq!(safe_relative("a\\b").as_deref(), Some("a/b"));
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "C:/x", "a\0b", "."] {
            assert_eq!(safe_relative(bad), None, "{bad}");
        }
    }
}
