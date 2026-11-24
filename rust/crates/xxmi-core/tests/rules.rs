//! Lints the repository's rule fixtures and compares against the golden output checked in:
//! fixtures/rules/XMnnn/{bad,good}(.ini|/) with expected.json (and fixed.ini when
//! the rule has quick fixes), and fixtures/xxmi-1.3.1/*.expected.json.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use xxmi_core::diagnostic::Position;
use xxmi_core::lint::{LintOptions, lint_paths};
use xxmi_core::paths::{display_path, resolve_path};
use xxmi_core::profile;
use xxmi_core::workspace::snapshot::Snapshots;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures")
}

fn read_json(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("golden file is a JSON array")
}

/// Fixtures must not depend on the bundled ZZMI library, so no snapshots.
fn options(rules: Option<Vec<String>>) -> LintOptions {
    LintOptions {
        rules,
        snapshots: Snapshots::Bundled(&[]),
        ..LintOptions::bundled()
    }
}

/// Diagnostics as the golden files record them, with paths relative to `dir`.
fn diagnose(path: &Path, dir: &Path, rules: Option<Vec<String>>) -> Vec<Value> {
    let result = lint_paths(&[path.to_path_buf()], &options(rules)).expect("lint runs");
    let base = resolve_path(dir);

    let mut out = Vec::new();
    for file in &result.files {
        for d in &file.diagnostics {
            let mut value = serde_json::to_value(d).expect("diagnostics serialize");
            value["path"] = json!(display_path(&file.path, &base));
            out.push(value);
        }
    }
    out
}

fn variant(dir: &Path, name: &str) -> PathBuf {
    [dir.join(format!("{name}.ini")), dir.join(name)]
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| panic!("{} {name} fixture", dir.display()))
}

fn offset(text: &str, position: &Value) -> usize {
    let line = position["line"].as_u64().unwrap() as usize;
    let mut column = position["character"].as_u64().unwrap() as usize;
    let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();

    let mut end = start;
    for c in text[start..].chars() {
        if column == 0 {
            break;
        }
        column = column.saturating_sub(c.len_utf16());
        end += c.len_utf8();
    }
    end
}

/// Applies every quick fix, last first so earlier offsets stay valid.
fn apply_fixes(text: &str, diagnostics: &[Value]) -> String {
    let mut edits: Vec<(usize, usize, String)> = diagnostics
        .iter()
        .flat_map(|d| d["fix"]["edits"].as_array().cloned().unwrap_or_default())
        .map(|e| {
            let start = offset(text, &e["range"]["start"]);
            let end = offset(text, &e["range"]["end"]);
            (start, end, e["newText"].as_str().unwrap().to_string())
        })
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));

    let mut out = text.to_string();
    for (start, end, new_text) in edits {
        out.replace_range(start..end, &new_text);
    }
    out
}

fn check_bad(id: &str) {
    let dir = fixtures().join("rules").join(id);
    let bad = variant(&dir, "bad");
    let actual = diagnose(&bad, &dir, Some(vec![id.to_string()]));

    assert!(
        !actual.is_empty(),
        "{id}: the bad fixture reports something"
    );
    assert_eq!(actual, read_json(&dir.join("expected.json")), "{id} bad");

    let fixed = dir.join("fixed.ini");
    let emits_fix = actual.iter().any(|d| d.get("fix").is_some());
    let rule = xxmi_core::rules::RULES.iter().find(|r| r.id == id).unwrap();
    assert!(
        rule.has_fix || !emits_fix,
        "{id}: emits a fix without has_fix"
    );
    if emits_fix {
        let before = fs::read_to_string(&bad).expect("bad.ini");
        assert_eq!(
            apply_fixes(&before, &actual),
            fs::read_to_string(&fixed).expect("fixed.ini")
        );
    } else {
        assert!(!fixed.exists(), "{id}: fixed.ini without quick fixes");
    }
}

fn check_good(id: &str) {
    let dir = fixtures().join("rules").join(id);
    let good = variant(&dir, "good");
    assert_eq!(
        diagnose(&good, &dir, Some(vec![id.to_string()])),
        Vec::<Value>::new(),
        "{id} good"
    );
}

/// A fix must not leave the finding behind, or applying it again would never settle.
#[test]
fn xm403_fix_is_idempotent() {
    let dir = fixtures().join("rules/XM403");
    let fixed = dir.join("fixed.ini");

    let actual = diagnose(&fixed, &dir, Some(vec!["XM403".to_string()]));

    assert_eq!(actual, Vec::<Value>::new());
}

macro_rules! rule_tests {
    ($($name:ident => $id:literal),* $(,)?) => {
        mod bad { $(#[test] fn $name() { super::check_bad($id) })* }
        mod good { $(#[test] fn $name() { super::check_good($id) })* }
    };
}

rule_tests! {
    xm001 => "XM001", xm002 => "XM002", xm003 => "XM003", xm005 => "XM005", xm006 => "XM006",
    xm101 => "XM101", xm102 => "XM102", xm103 => "XM103", xm104 => "XM104", xm105 => "XM105",
    xm106 => "XM106", xm107 => "XM107", xm108 => "XM108", xm109 => "XM109",
    xm110 => "XM110", xm111 => "XM111", xm112 => "XM112", xm113 => "XM113", xm114 => "XM114", xm115 => "XM115",
    xm201 => "XM201", xm202 => "XM202", xm203 => "XM203", xm204 => "XM204", xm205 => "XM205",
    xm206 => "XM206", xm207 => "XM207", xm208 => "XM208", xm209 => "XM209", xm210 => "XM210",
    xm211 => "XM211", xm212 => "XM212", xm307 => "XM307",
    xm403 => "XM403",
}

/// The XXMI DLL has everything the GIMI DLL lacks, so the same fixture is clean under its profiles.
#[test]
fn xm307_is_silent_outside_the_gimi_profile() {
    let dir = fixtures().join("rules/XM307");
    for id in ["xxmi", "zzmi"] {
        let options = LintOptions {
            profile: profile::by_id(id),
            ..options(Some(vec!["XM307".to_string()]))
        };
        let result = lint_paths(&[dir.join("bad.ini")], &options).expect("lint runs");
        assert!(
            result.files.iter().all(|f| f.diagnostics.is_empty()),
            "{id}"
        );
    }
}

/// Windows checkouts and editors hand the tooling CRLF files; diagnostics must not change,
/// except that fixes insert the file's own line break.
#[test]
fn crlf_files_report_the_same_diagnostics_as_lf() {
    let scratch = std::env::temp_dir().join(format!("xxmi-crlf-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();

    let mut checked = 0;
    for entry in fs::read_dir(fixtures().join("rules")).unwrap() {
        let dir = entry.unwrap().path();
        let bad = dir.join("bad.ini");
        if !bad.is_file() {
            continue;
        }
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();

        let crlf = scratch.join("bad.ini");
        fs::write(&crlf, to_crlf(&fs::read(&bad).unwrap())).unwrap();
        copy_profile_config(&dir, &scratch);
        let rules = Some(vec![id.clone()]);

        let actual = diagnose(&crlf, &scratch, rules.clone());
        assert_eq!(
            normalise_fix_text(actual.clone()),
            diagnose(&bad, &dir, rules),
            "{id} with CRLF"
        );
        if let Ok(text) = fs::read_to_string(&crlf) {
            assert_no_bare_lf(&id, &apply_fixes(&text, &actual));
        }
        checked += 1;
    }
    fs::remove_dir_all(&scratch).unwrap();
    assert!(checked > 15, "only {checked} single-file fixtures found");
}

/// Applying every fixture's fixes to a CRLF copy gives fixed.ini with CRLF breaks only.
#[test]
fn quick_fixes_on_crlf_files_keep_the_files_line_breaks() {
    let scratch = std::env::temp_dir().join(format!("xxmi-crlf-fix-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();

    let mut checked = 0;
    for entry in fs::read_dir(fixtures().join("rules")).unwrap() {
        let dir = entry.unwrap().path();
        let (bad, fixed) = (dir.join("bad.ini"), dir.join("fixed.ini"));
        if !bad.is_file() || !fixed.is_file() {
            continue;
        }
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();

        let crlf = scratch.join("bad.ini");
        fs::write(&crlf, to_crlf(&fs::read(&bad).unwrap())).unwrap();
        let before = fs::read_to_string(&crlf).unwrap();
        let diagnostics = diagnose(&crlf, &scratch, Some(vec![id.clone()]));

        let after = apply_fixes(&before, &diagnostics);
        assert_no_bare_lf(&id, &after);
        let expected = String::from_utf8(to_crlf(&fs::read(&fixed).unwrap())).unwrap();
        assert_eq!(after, expected, "{id} fixed with CRLF");
        checked += 1;
    }
    fs::remove_dir_all(&scratch).unwrap();
    assert!(
        checked > 5,
        "only {checked} fixtures with quick fixes found"
    );
}

/// A fixture's `xxmi.toml` picks its profile, so the CRLF copy needs it too (and no stale one).
fn copy_profile_config(fixture: &Path, scratch: &Path) {
    let copy = scratch.join("xxmi.toml");
    if fs::copy(fixture.join("xxmi.toml"), &copy).is_err() {
        let _ = fs::remove_file(copy);
    }
}

/// Fix texts with CRLF breaks turned back into LF, to compare against the LF run.
fn normalise_fix_text(mut diagnostics: Vec<Value>) -> Vec<Value> {
    for d in &mut diagnostics {
        let Some(edits) = d.get_mut("fix").and_then(|f| f["edits"].as_array_mut()) else {
            continue;
        };
        for edit in edits {
            let text = edit["newText"].as_str().unwrap().replace("\r\n", "\n");
            edit["newText"] = json!(text);
        }
    }
    diagnostics
}

fn assert_no_bare_lf(id: &str, fixed: &str) {
    let bare = fixed
        .match_indices('\n')
        .any(|(i, _)| !fixed[..i].ends_with('\r'));
    assert!(!bare, "{id}: a fix left a bare LF in a CRLF file");
}

fn to_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 16);
    for &b in bytes {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

#[test]
fn every_rule_has_a_fixture_directory() {
    let mut dirs: Vec<String> = fs::read_dir(fixtures().join("rules"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("XM"))
        .collect();
    dirs.sort();

    let ids: Vec<&str> = xxmi_core::rules::RULES.iter().map(|r| r.id).collect();
    assert_eq!(dirs, ids);
}

#[test]
fn xxmi_1_3_1_fixtures_match_the_golden_output() {
    let dir = fixtures().join("xxmi-1.3.1");
    let mut checked = 0;

    for entry in fs::read_dir(&dir).expect("fixture directory exists") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ini") {
            continue;
        }

        let golden = path.with_extension("expected.json");
        let expected = if golden.exists() {
            read_json(&golden)
        } else {
            Vec::new()
        };
        let actual: Vec<Value> = diagnose(&path, &dir, None)
            .iter()
            .map(|d| json!({ "id": d["id"], "message": d["message"], "range": d["range"] }))
            .collect();
        assert_eq!(actual, expected, "{}", path.display());
        checked += 1;
    }
    assert_eq!(
        checked, 3,
        "clean, matchtextures-missing and run-misspelled"
    );
}

/// fixtures/ownership: every line a dedicated rule names is reported by that rule alone,
/// not also as XM001, XM102, XM103, XM109, XM110 or XM204.
#[test]
fn stray_lines_are_reported_once_under_their_own_rule() {
    let dir = fixtures().join("ownership");
    let found: Vec<String> = diagnose(&dir.join("mixed.ini"), &dir, None)
        .iter()
        .filter(|d| {
            let id = d["id"].as_str().unwrap();
            id.starts_with("XM0")
                || id == "XM403"
                || matches!(
                    id,
                    "XM102"
                        | "XM103"
                        | "XM109"
                        | "XM110"
                        | "XM111"
                        | "XM204"
                        | "XM209"
                        | "XM210"
                        | "XM211"
                )
        })
        .map(|d| {
            format!(
                "{}:{}",
                d["range"]["start"]["line"].as_u64().unwrap() + 1,
                d["id"].as_str().unwrap()
            )
        })
        .collect();

    let expected: Vec<String> =
        serde_json::from_str(&fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn position_type_is_utf16_columns() {
    // Guards the unit the golden ranges are written in.
    let p = Position {
        line: 0,
        character: 2,
    };
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        json!({ "line": 0, "character": 2 })
    );
}
