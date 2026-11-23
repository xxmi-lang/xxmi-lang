//! The lint output contract: files by path, then diagnostics by position and rule id,
//! and `require_package` skipping files outside any package.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use xxmi_core::lint::format::format_json;
use xxmi_core::lint::{ConfigLoader, LintOptions, lint_paths, lint_texts};
use xxmi_core::paths::resolve_path;
use xxmi_core::workspace::snapshot::Snapshots;

fn options() -> LintOptions {
    LintOptions {
        snapshots: Snapshots::Bundled(&[]),
        ..LintOptions::bundled()
    }
}

/// Several diagnostics at different lines and rules, so the order is observable.
const MESSY: &str = "[Bogus]\n[Resource\\R]\nbogus_key = 1\n[Resource\\S]\nbogus_other = 1\n";

#[test]
fn files_sort_by_path_and_diagnostics_by_position_then_rule() {
    let files: Vec<(PathBuf, String)> = ["/v/b/z.ini", "/v/a/y.ini", "/v/B/x.ini", "/v/a/Y.ini"]
        .iter()
        .map(|path| (PathBuf::from(path), MESSY.to_string()))
        .collect();
    let result = lint_texts(&files, &options(), &ConfigLoader::disabled()).unwrap();
    let json = format_json(&result, &|path: &Path| path.to_string_lossy().into_owned()).unwrap();
    let document: Value = serde_json::from_str(&json).unwrap();

    let paths: Vec<&str> = document["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["/v/B/x.ini", "/v/a/Y.ini", "/v/a/y.ini", "/v/b/z.ini"]
    );

    for file in document["files"].as_array().unwrap() {
        let keys: Vec<(u64, u64, &str)> = file["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                let start = &d["range"]["start"];
                (
                    start["line"].as_u64().unwrap(),
                    start["character"].as_u64().unwrap(),
                    d["id"].as_str().unwrap(),
                )
            })
            .collect();
        assert!(keys.len() >= 3, "{keys:?}");
        assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]), "{keys:?}");
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xxmi-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A package with one mod, and a loose file beside it that no d3dx.ini covers.
fn package_and_loose_file(dir: &Path) {
    fs::create_dir_all(dir.join("pkg/Mods")).unwrap();
    fs::write(dir.join("pkg/d3dx.ini"), "[Constants]\n").unwrap();
    fs::write(dir.join("pkg/Mods/mod.ini"), MESSY).unwrap();
    fs::create_dir_all(dir.join("loose")).unwrap();
    fs::write(dir.join("loose/mod.ini"), MESSY).unwrap();
}

fn linted_names(dir: &Path, require_package: bool) -> Vec<String> {
    let options = LintOptions {
        require_package,
        ..options()
    };
    let result = lint_paths(&[dir.to_path_buf()], &options).unwrap();
    let mut names: Vec<String> = result
        .files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(resolve_path(dir))
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    names.sort();
    assert_eq!(result.summary.files, names.len());
    names
}

#[test]
fn require_package_skips_files_outside_any_package() {
    let dir = scratch("require-package");
    package_and_loose_file(&dir);

    assert_eq!(
        linted_names(&dir, false),
        ["loose/mod.ini", "pkg/Mods/mod.ini", "pkg/d3dx.ini"]
    );
    assert_eq!(
        linted_names(&dir, true),
        ["pkg/Mods/mod.ini", "pkg/d3dx.ini"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn many_packages_lint_in_one_run() {
    let dir = scratch("many-packages");
    for n in 0..3 {
        let package = dir.join(format!("p{n}"));
        fs::create_dir_all(package.join("Mods")).unwrap();
        fs::write(package.join("d3dx.ini"), "[Constants]\n").unwrap();
        fs::write(package.join("Mods/mod.ini"), MESSY).unwrap();
    }

    let options = LintOptions {
        require_package: true,
        ..options()
    };
    let result = lint_paths(std::slice::from_ref(&dir), &options).unwrap();
    assert_eq!(result.summary.files, 6);
    let ids = result
        .files
        .iter()
        .flat_map(|f| &f.diagnostics)
        .map(|d| d.id);
    assert!(
        ids.clone().all(|id| id != "XM107"),
        "packages must not clash"
    );
    for n in 0..3 {
        let needle = format!("p{n}");
        let in_package = result
            .files
            .iter()
            .filter(|f| f.path.to_string_lossy().contains(&needle));
        assert_eq!(in_package.count(), 2, "{needle}");
    }
    fs::remove_dir_all(&dir).unwrap();
}
