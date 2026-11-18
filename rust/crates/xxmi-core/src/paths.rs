//! Paths inside inis follow Windows rules whatever OS the tooling runs on: `\` and `/` are both
//! separators and lookup is case-insensitive (port of packages/core/src/paths.ts).
//! Never hand an ini path straight to the file system.

use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

/// Splits an ini path into segments, dropping empty and `.` segments.
fn ini_path_segments(ini_path: &str) -> impl Iterator<Item = &str> {
    ini_path
        .split(['\\', '/'])
        .filter(|segment| !segment.is_empty() && *segment != ".")
}

/// Drops the `\\?\` verbatim marker that `canonicalize` adds on Windows, which would otherwise
/// make `\\?\C:\a` and `C:\a` different roots (`\\?\UNC\h\s` becomes `\\h\s`).
pub fn strip_verbatim(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

/// A path component as compared by `relative_path`: the prefix (drive letter, UNC share) ignores
/// verbatim form and case, as Windows does.
fn component_key(component: Component) -> String {
    let text = component.as_os_str().to_string_lossy();
    match component {
        Component::Prefix(_) => strip_verbatim(&text).to_lowercase(),
        _ if cfg!(windows) => text.to_lowercase(),
        _ => text.into_owned(),
    }
}

/// An absolute, lexically normalised path (`path.resolve`): relative paths start at the working
/// directory, `.` and `..` segments are folded.
pub fn resolve_path(path: &Path) -> PathBuf {
    let path = &if cfg!(windows) {
        PathBuf::from(strip_verbatim(&path.to_string_lossy()))
    } else {
        path.to_path_buf()
    };
    let base = if path.is_absolute() {
        PathBuf::new()
    } else {
        env::current_dir().unwrap_or_default()
    };

    let mut out = PathBuf::new();
    for component in base.components().chain(path.components()) {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// `path.relative(from, to)` for two resolved paths, with the host separator.
pub fn relative_path(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| component_key(**a) == component_key(**b))
        .count();

    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join(std::path::MAIN_SEPARATOR_STR)
}

/// A host path shown to users: relative to `base` when inside it, always with `/`.
pub fn display_path(path: &Path, base: &Path) -> String {
    let relative = relative_path(base, path);
    let shown = if !relative.is_empty() && !relative.starts_with("..") {
        relative
    } else {
        path.to_string_lossy().into_owned()
    };
    shown.replace(std::path::MAIN_SEPARATOR, "/")
}

/// Resolves an ini-relative path against a host directory the way Windows would: segment by
/// segment, matching names case-insensitively. `..` goes up. Directory listings are cached per
/// resolver, so one resolver should serve a single lint run.
#[derive(Default)]
pub struct PathResolver {
    listings: RefCell<HashMap<PathBuf, Option<Rc<Vec<String>>>>>,
}

impl PathResolver {
    pub fn resolve(&self, base_dir: &Path, ini_path: &str) -> Option<PathBuf> {
        let mut current = base_dir.to_path_buf();
        for segment in ini_path_segments(ini_path) {
            if segment == ".." {
                current.pop();
                continue;
            }

            let entries = self.list(&current)?;
            let lower = segment.to_lowercase();
            let found = entries
                .iter()
                .find(|e| e.as_str() == segment)
                .or_else(|| entries.iter().find(|e| e.to_lowercase() == lower))?;
            current.push(found);
        }
        Some(current)
    }

    pub fn is_directory(&self, path: &Path) -> bool {
        fs::metadata(path).is_ok_and(|m| m.is_dir())
    }

    /// Directory entries (names), or `None` if `dir` can't be read.
    pub fn list(&self, dir: &Path) -> Option<Rc<Vec<String>>> {
        if let Some(cached) = self.listings.borrow().get(dir) {
            return cached.clone();
        }

        let entries = fs::read_dir(dir).ok().map(|read| {
            let mut names: Vec<String> = read
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            // Node's readdir is sorted (libuv's scandir), and completion order follows it.
            names.sort();
            Rc::new(names)
        });
        self.listings
            .borrow_mut()
            .insert(dir.to_path_buf(), entries.clone());
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/` alone is not absolute on Windows (it takes the working directory's drive), so the
    /// expectation needs a drive there.
    fn absolute(rest: &str) -> PathBuf {
        Path::new(if cfg!(windows) { r"C:\" } else { "/" }).join(rest)
    }

    #[test]
    fn segments_drop_empty_and_dot() {
        let segments: Vec<_> = ini_path_segments(".\\a//b\\.\\c").collect();
        assert_eq!(segments, ["a", "b", "c"]);
    }

    #[test]
    fn verbatim_prefixes_are_dropped() {
        assert_eq!(strip_verbatim(r"\\?\D:\a\b"), r"D:\a\b");
        assert_eq!(strip_verbatim(r"\\?\UNC\host\share\a"), r"\\host\share\a");
        assert_eq!(strip_verbatim(r"D:\a"), r"D:\a");
    }

    #[test]
    fn resolve_folds_parent_segments() {
        assert_eq!(resolve_path(&absolute("a/b/../c/./d")), absolute("a/c/d"));
    }

    #[test]
    fn display_is_relative_inside_the_base_and_absolute_outside() {
        let base = Path::new("/work");
        assert_eq!(display_path(Path::new("/work/a/b.ini"), base), "a/b.ini");
        assert_eq!(
            display_path(Path::new("/other/b.ini"), base),
            "/other/b.ini"
        );
        assert_eq!(display_path(Path::new("/work"), base), "/work");
    }
}
