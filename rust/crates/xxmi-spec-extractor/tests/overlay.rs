//! The overlay merge on the synthetic DLL pair.

use std::path::Path;

use xxmi_spec_extractor::overlay::{OverlayFile, apply_overlays};
use xxmi_spec_extractor::{ExtractOptions, GitInfo, Spec, extract_spec};

fn spec() -> Spec {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/spec-extractor");
    let git = GitInfo {
        repository: String::new(),
        commit: String::new(),
        commit_date: String::new(),
    };
    extract_spec(ExtractOptions {
        dll_root: &fixtures.join("xxmi"),
        baseline_root: &fixtures.join("vanilla"),
        dll_git: Some(git.clone()),
        baseline_git: Some(git),
        gimi_root: None,
        gimi_git: None,
    })
    .unwrap()
    .spec
}

fn overlay(text: &str) -> Vec<OverlayFile> {
    vec![OverlayFile {
        file: "test.toml".to_string(),
        overlay: text.parse().unwrap(),
    }]
}

#[test]
fn documents_existing_entries_case_insensitively() {
    let mut spec = spec();
    let problems = apply_overlays(
        &mut spec,
        &overlay(
            r#"
[commands.RUN]
doc = "Runs a list."

[sections.resource.keys.TYPE]
doc = "Kind."
valueType = "string"

[operators."<<"]
note = "shift"
"#,
        ),
    );
    assert!(problems.is_empty(), "{problems:?}");

    let run = spec.body.commands.iter().find(|c| c.name == "run").unwrap();
    assert_eq!(
        run.docs.0,
        [("doc".to_string(), "Runs a list.".to_string())]
    );
    let resource = spec
        .body
        .sections
        .iter()
        .find(|s| s.name == "Resource")
        .unwrap();
    let key = resource.keys.iter().find(|k| k.name == "type").unwrap();
    assert_eq!(key.value_type, "string");
    assert_eq!(key.docs.0, [("doc".to_string(), "Kind.".to_string())]);
}

#[test]
fn adds_manual_entries_and_reports_everything_else() {
    let mut spec = spec();
    let problems = apply_overlays(
        &mut spec,
        &overlay(
            r#"
[functions.fake]
source = "manual"
note = "Added by hand."

[functions.missing]
doc = "x"

[commands.fake]
source = "manual"
kind = "general"

[commands.run]
source = "manual"
bogus = "x"
doc = 1

[widgets.a]
doc = "x"
"#,
        ),
    );
    assert!(
        spec.body
            .functions
            .iter()
            .any(|f| f.name == "fake" && f.source == "manual")
    );
    assert_eq!(
        problems,
        [
            "test.toml: functions.missing: not in the generated spec (add `source = \"manual\"` and a `note` if intended)",
            "test.toml: commands.fake: manual entries need a `note` saying why they exist",
            "test.toml: commands.run: `source` can only be set on new (`source = \"manual\"`) entries",
            "test.toml: commands.run: field `bogus` can't be set by an overlay",
            "test.toml: commands.run: `doc` must be a string",
            "test.toml: widgets: unknown overlay group",
        ]
    );
}
