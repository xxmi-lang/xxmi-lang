//! `lint_text`: the `--stdin` flow, where the text stands in for a file that need not exist.

use std::path::Path;

use xxmi_core::encoding::Encoding;
use xxmi_core::lint::{LintOptions, LintResult, lint_text};

fn lint(text: &str, encoding: Encoding) -> LintResult {
    let options = LintOptions {
        rules: Some(vec!["XM101".to_string(), "XM002".to_string()]),
        snapshots: &[],
        ..LintOptions::bundled()
    };
    lint_text(
        Path::new("/nonexistent/mod/mod.ini"),
        text.to_string(),
        encoding,
        &options,
    )
    .expect("lint runs")
}

fn ids(result: &LintResult) -> Vec<&'static str> {
    result.files[0].diagnostics.iter().map(|d| d.id).collect()
}

#[test]
fn text_is_linted_under_the_given_path() {
    let result = lint("[Bogus]\n", Encoding::Utf8);
    assert_eq!(ids(&result), ["XM101"]);
    assert!(result.files[0].path.ends_with("mod/mod.ini"));
}

#[test]
fn inline_directive_suppresses_the_next_line() {
    let result = lint("; xxmi-disable-next-line XM101\n[Bogus]\n", Encoding::Utf8);
    assert!(ids(&result).is_empty());
}

#[test]
fn disable_region_runs_until_enable() {
    let text = "; xxmi-disable XM101\n[A]\n; xxmi-enable\n[B]\n";
    assert_eq!(ids(&lint(text, Encoding::Utf8)), ["XM101"]);
}

#[test]
fn undecodable_input_is_reported_as_xm002() {
    let result = lint("[Constants]\n", Encoding::Unknown);
    assert_eq!(ids(&result), ["XM002"]);
    assert_eq!(result.files[0].encoding, Encoding::Unknown);
}
