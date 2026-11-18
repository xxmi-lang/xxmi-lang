//! Lints the repository's rule fixtures with the ported rules and compares against the golden
//! output the TypeScript tests check in (fixtures/rules/XMnnn/expected.json and
//! fixtures/xxmi-1.3.1/*.expected.json), restricted to the two ported rule ids.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use xxmi_core::document::Document;
use xxmi_core::encoding::decode_text;
use xxmi_core::lint::lint;

const PORTED: [&str; 2] = ["XM001", "XM102"];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures")
}

fn read_json(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("golden file is a JSON array")
}

fn lint_file(path: &Path, rules: &[String]) -> Vec<Value> {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (text, _) = decode_text(&bytes);
    lint(&Document::new(text), Some(rules))
        .iter()
        .map(|d| serde_json::to_value(d).expect("diagnostics serialize"))
        .collect()
}

fn is_ported(entry: &Value) -> bool {
    PORTED.iter().any(|id| entry["id"] == *id)
}

/// Keeps only the fields the golden file records.
fn project(entry: &Value, fields: &[&str]) -> Value {
    let kept = fields
        .iter()
        .filter_map(|f| entry.get(*f).map(|v| (f.to_string(), v.clone())));
    Value::Object(kept.collect())
}

fn assert_matches(what: &str, actual: &[Value], expected: &[Value], fields: &[&str]) {
    let actual: Vec<Value> = actual.iter().map(|e| project(e, fields)).collect();
    let expected: Vec<Value> = expected
        .iter()
        .filter(|e| is_ported(e))
        .map(|e| project(e, fields))
        .collect();
    assert_eq!(actual, expected, "{what}");
}

#[test]
fn rule_fixtures_match_the_golden_output() {
    for id in PORTED {
        let dir = fixtures().join("rules").join(id);
        let rules = [id.to_string()];
        let fields = ["id", "severity", "message", "range", "fix"];

        let bad = lint_file(&dir.join("bad.ini"), &rules);
        assert!(!bad.is_empty(), "{id}: the bad fixture reports something");
        assert_matches(
            &format!("{id} bad.ini"),
            &bad,
            &read_json(&dir.join("expected.json")),
            &fields,
        );

        let good = lint_file(&dir.join("good.ini"), &rules);
        assert_eq!(good, Vec::<Value>::new(), "{id} good.ini");
    }
}

#[test]
fn xxmi_1_3_1_fixtures_match_the_golden_output() {
    let dir = fixtures().join("xxmi-1.3.1");
    let all = ["XM001".to_string(), "XM102".to_string()];
    let mut checked = 0;

    for entry in fs::read_dir(&dir).expect("fixture directory exists") {
        let path = entry.expect("readable entry").path();
        if path.extension().is_none_or(|e| e != "ini") {
            continue;
        }

        let golden = path.with_extension("expected.json");
        let expected = if golden.exists() {
            read_json(&golden)
        } else {
            Vec::new()
        };
        assert_matches(
            &path.display().to_string(),
            &lint_file(&path, &all),
            &expected,
            &["id", "message", "range"],
        );
        checked += 1;
    }
    assert_eq!(
        checked, 3,
        "clean, matchtextures-missing and run-misspelled"
    );
}

#[test]
fn fix_edits_turn_the_bad_fixture_into_the_fixed_one() {
    let dir = fixtures().join("rules/XM102");
    let before = fs::read_to_string(dir.join("bad.ini")).expect("bad.ini");
    let fixed = fs::read_to_string(dir.join("fixed.ini")).expect("fixed.ini");

    let mut doc = Document::new(before);
    let edits: Vec<_> = lint_file(&dir.join("bad.ini"), &["XM102".to_string()])
        .iter()
        .flat_map(|d| d["fix"]["edits"].as_array().cloned().unwrap_or_default())
        .collect();
    let mut changes: Vec<_> = edits
        .iter()
        .map(|e| json!({"range": e["range"], "text": e["newText"]}))
        .collect();
    // Later positions first, so earlier edits don't shift them.
    changes.sort_by_key(|c| {
        std::cmp::Reverse((
            c["range"]["start"]["line"].as_u64(),
            c["range"]["start"]["character"].as_u64(),
        ))
    });

    let changes: Vec<_> = changes.iter().map(to_change).collect();
    doc.apply_changes(&changes);
    assert_eq!(doc.text(), fixed);
}

fn to_change(value: &Value) -> xxmi_core::document::TextChange {
    let position = |p: &Value| xxmi_core::diagnostic::Position {
        line: p["line"].as_u64().expect("line") as u32,
        character: p["character"].as_u64().expect("character") as u32,
    };
    let range = &value["range"];
    xxmi_core::document::TextChange {
        range: Some(xxmi_core::diagnostic::Range {
            start: position(&range["start"]),
            end: position(&range["end"]),
        }),
        text: value["text"].as_str().expect("text").to_string(),
    }
}
