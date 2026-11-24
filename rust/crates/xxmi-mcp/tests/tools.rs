//! Each tool through /mcp: a happy path and a bad input.

mod support;

use serde_json::json;
use support::{app, call, data_of, text_of};

#[tokio::test]
async fn spec_lookup_finds_an_entry_and_reports_the_spec() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let result = call(
        &app,
        "spec_lookup",
        json!({ "name": "match_dhash", "format": "json" }),
    )
    .await;

    assert_ne!(result["isError"], json!(true), "{result}");
    assert!(!data_of(&result)["entries"].as_array().unwrap().is_empty());
    assert!(data_of(&result)["spec"].is_object());
}

#[tokio::test]
async fn spec_lookup_rejects_a_missing_name() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let result = call(&app, "spec_lookup", json!({})).await;

    assert_eq!(result["isError"], json!(true));
    assert!(text_of(&result).contains("Invalid arguments for tool spec_lookup"));
}

#[tokio::test]
async fn spec_list_pages_a_kind_and_rejects_an_unknown_one() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let ok = call(
        &app,
        "spec_list",
        json!({ "kind": "section", "format": "json" }),
    )
    .await;
    let bad = call(&app, "spec_list", json!({ "kind": "nonsense" })).await;

    assert!(
        !data_of(&ok)["items"].as_array().unwrap().is_empty(),
        "{ok}"
    );
    assert_eq!(bad["isError"], json!(true));
}

#[tokio::test]
async fn library_list_and_symbol_answer_from_the_indexed_package() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let list = call(&app, "library_list", json!({ "game": "zzmi" })).await;
    let symbol = call(
        &app,
        "library_symbol",
        json!({ "qualified_name": "CommandList\\ZZMI\\SetTextures", "game": "zzmi" }),
    )
    .await;
    let bad = call(&app, "library_list", json!({ "game": "nope" })).await;

    assert_ne!(list["isError"], json!(true), "{list}");
    assert!(text_of(&symbol).contains("SetTextures"), "{symbol}");
    assert_eq!(bad["isError"], json!(true));
}

#[tokio::test]
async fn find_implementation_searches_the_dll_source() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let ok = call(
        &app,
        "find_implementation",
        json!({ "feature": "match_dhash", "format": "json" }),
    )
    .await;
    let bad = call(
        &app,
        "find_implementation",
        json!({ "feature": "x", "limit": 0 }),
    )
    .await;

    assert_ne!(ok["isError"], json!(true), "{ok}");
    assert!(data_of(&ok)["dll"]["sha"].is_string());
    assert_eq!(bad["isError"], json!(true));
}

#[tokio::test]
async fn search_source_shells_out_and_rejects_unknown_repos() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let ok = call(
        &app,
        "search_source",
        json!({ "repo": "dll", "query": "SetTextures|match_dhash" }),
    )
    .await;
    let bad = call(
        &app,
        "search_source",
        json!({ "repo": "nope", "query": "x" }),
    )
    .await;

    assert_ne!(ok["isError"], json!(true), "{ok}");
    assert_eq!(bad["isError"], json!(true));
}

#[tokio::test]
async fn read_source_returns_numbered_lines_and_rejects_escapes() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let ok = call(
        &app,
        "read_source",
        json!({ "repo": "dll", "path": "README.md", "start_line": 1, "end_line": 2, "format": "json" }),
    )
    .await;
    let bad = call(
        &app,
        "read_source",
        json!({ "repo": "dll", "path": "../secret", "start_line": 1, "end_line": 2 }),
    )
    .await;

    assert_ne!(ok["isError"], json!(true), "{ok}");
    assert!(data_of(&ok)["permalink"].is_string());
    assert_eq!(bad["isError"], json!(true));
}

#[tokio::test]
async fn lint_ini_reports_diagnostics_and_rejects_non_ini_files() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;
    let files = json!({ "mod.ini": "[Present]\nrun = CommandList\\Missing\n" });

    let ok = call(
        &app,
        "lint_ini",
        json!({ "files": files, "format": "json" }),
    )
    .await;
    let bad = call(&app, "lint_ini", json!({ "files": { "mod.txt": "" } })).await;

    assert_ne!(ok["isError"], json!(true), "{ok}");
    assert!(data_of(&ok)["summary"].is_object());
    assert_eq!(bad["isError"], json!(true));
}

#[tokio::test]
async fn lint_ini_without_a_package_resolves_zzmi_library_calls() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;
    let files = json!({ "mod.ini": "[Present]\nrun = CommandList\\ZZMI\\SetTextures\n" });

    let result = call(
        &app,
        "lint_ini",
        json!({ "files": files, "format": "json" }),
    )
    .await;

    let data = data_of(&result);
    let ids: Vec<&str> = data["files"][0]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["id"].as_str())
        .collect();
    assert!(!ids.contains(&"XM201"), "{data}");
}

#[tokio::test]
async fn list_versions_lists_every_tracked_repo() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), &[]).await;

    let ok = call(&app, "list_versions", json!({ "format": "json" })).await;
    let bad = call(&app, "list_versions", json!({ "format": "xml" })).await;

    assert_eq!(data_of(&ok)["repos"].as_array().unwrap().len(), 4, "{ok}");
    assert_eq!(bad["isError"], json!(true));
}
