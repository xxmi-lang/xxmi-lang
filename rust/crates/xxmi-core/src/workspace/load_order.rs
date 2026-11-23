//! The order the DLL loads files in (`ParseIniFilesRecursive`, IniHandler.cpp): within a folder,
//! `.ini` files come before subfolders, and names sort with `_wcsicmp`.

use std::cmp::Ordering;
use std::path::{Component, Path};

/// Orders two paths relative to the same root; the lesser one is parsed first.
pub fn compare_load_order(a: &Path, b: &Path) -> Ordering {
    let a: Vec<_> = a.components().collect();
    let b: Vec<_> = b.components().collect();

    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        let (x_is_file, y_is_file) = (i + 1 == a.len(), i + 1 == b.len());
        if x_is_file != y_is_file {
            return if x_is_file {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }

        let order = compare_names(*x, *y);
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

/// `path` relative to `root`, or unchanged when it is outside it or there is no root.
pub fn relative_to_root<'a>(path: &'a Path, root: Option<&Path>) -> &'a Path {
    root.and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
}

/// `_wcsicmp` lower-cases each UTF-16 unit, then compares the units.
fn compare_names(a: Component, b: Component) -> Ordering {
    let units = |name: Component| {
        name.as_os_str()
            .to_string_lossy()
            .to_lowercase()
            .encode_utf16()
            .collect::<Vec<_>>()
    };
    units(a).cmp(&units(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(a: &str, b: &str) -> Ordering {
        compare_load_order(Path::new(a), Path::new(b))
    }

    #[test]
    fn names_compare_case_insensitively() {
        assert_eq!(order("a.ini", "B.ini"), Ordering::Less);
        assert_eq!(order("B.ini", "a.ini"), Ordering::Greater);
    }

    #[test]
    fn files_come_before_subfolders() {
        assert_eq!(order("z.ini", "sub/a.ini"), Ordering::Less);
        assert_eq!(order("sub/a.ini", "z.ini"), Ordering::Greater);
    }

    #[test]
    fn folders_compare_case_insensitively() {
        assert_eq!(order("Sub/x.ini", "sub2/y.ini"), Ordering::Less);
        assert_eq!(order("sub/b.ini", "Sub/a.ini"), Ordering::Greater);
    }
}
