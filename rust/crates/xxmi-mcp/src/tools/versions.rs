//! `list_versions`: indexed repos and refs, and the spec status of each DLL version.

use serde_json::{Map, Value, json};

use crate::catalog::{RepoState, VersionRef, latest_prerelease, latest_release};
use crate::repos::{RepoRole, TRACKED_REPOS, TrackedRepo};
use crate::specs::short;

use super::shared::{ToolContext, ToolOutput, ToolResult, to_data};

fn summary(version: Option<&VersionRef>) -> Value {
    version.map_or(
        Value::Null,
        |v| json!({ "name": v.name, "sha": v.sha, "publishedAt": v.published_at }),
    )
}

/// A DLL version with its spec status (`spec`, and `specError` when the build failed).
fn with_spec_status(ctx: &ToolContext, version: &VersionRef) -> Value {
    let status = ctx.specs.status(&version.sha);
    let mut data = to_data(version);
    if let Value::Object(map) = &mut data {
        map.insert("spec".into(), status.state.into());
        if let Some(error) = status.error {
            map.insert("specError".into(), error.into());
        }
    }
    data
}

fn decorate(ctx: &ToolContext, repo: &TrackedRepo, version: &VersionRef) -> Value {
    if repo.role == RepoRole::Dll {
        with_spec_status(ctx, version)
    } else {
        to_data(version)
    }
}

fn describe_repo(ctx: &ToolContext, repo: &TrackedRepo) -> Value {
    let state: Option<&RepoState> = ctx.catalog.repo_state(repo);
    let default = state.map_or(Value::Null, |s| {
        let mut branch = decorate(ctx, repo, &s.branch);
        if let Value::Object(map) = &mut branch {
            map.insert("indexedAt".into(), s.indexed_at.clone().into());
        }
        branch
    });
    let tags: Vec<Value> = state
        .map(|s| s.tags.iter().map(|t| decorate(ctx, repo, t)).collect())
        .unwrap_or_default();

    json!({
        "repo": repo.full_name(),
        "alias": repo.alias,
        "role": repo.role.as_str(),
        "indexed": state.is_some(),
        "default": default,
        "tags": tags,
        "latestRelease": state.map_or(Value::Null, |s| summary(latest_release(s))),
        "latestPrerelease": state.map_or(Value::Null, |s| summary(latest_prerelease(s))),
    })
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `  latest release: v1.2.3 (2026-10-31)`; nothing when there is none.
fn latest_line(label: &str, found: &Value) -> Option<String> {
    if found.is_null() {
        return None;
    }
    let date = found
        .get("publishedAt")
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty());
    let date = date
        .map(|d| format!(" ({})", d.chars().take(10).collect::<String>()))
        .unwrap_or_default();
    Some(format!("  latest {label}: {}{date}", field(found, "name")))
}

/// `specs: master ok, v1.3.1 ok, v1.2.3 failed (reason)`, for the DLL repo only.
fn spec_status_line(repo: &Value) -> Option<String> {
    let default = repo.get("default").filter(|d| !d.is_null())?;
    if field(repo, "role") != "dll" {
        return None;
    }
    let tags = repo
        .get("tags")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let parts: Vec<String> = std::iter::once(default)
        .chain(tags)
        .map(|v| {
            let error = v
                .get("specError")
                .and_then(Value::as_str)
                .map(|e| format!(" ({e})"))
                .unwrap_or_default();
            format!("{} {}{error}", field(v, "name"), field(v, "spec"))
        })
        .collect();
    Some(format!("  specs: {}", parts.join(", ")))
}

fn repo_lines(repo: &Value) -> Vec<String> {
    let name = field(repo, "repo");
    let Some(default) = repo.get("default").filter(|d| !d.is_null()) else {
        return vec![format!("{name}: not indexed yet")];
    };

    let tags: Vec<&str> = repo
        .get("tags")
        .and_then(Value::as_array)
        .map(|t| t.iter().map(|v| field(v, "name")).collect())
        .unwrap_or_default();
    let tags = if tags.is_empty() {
        "none".to_string()
    } else {
        tags.join(", ")
    };
    let mut lines = vec![format!(
        "{name}: {} @ {} (indexed {}); tags: {tags}",
        field(default, "name"),
        short(field(default, "sha")),
        field(default, "indexedAt")
    )];
    lines.extend(latest_line("release", &repo["latestRelease"]));
    lines.extend(latest_line("pre-release", &repo["latestPrerelease"]));
    lines.extend(spec_status_line(repo));
    lines
}

pub fn list_versions(ctx: &ToolContext) -> ToolResult {
    let repos: Vec<Value> = TRACKED_REPOS
        .iter()
        .map(|r| describe_repo(ctx, r))
        .collect();
    let bundled = &ctx.specs.bundled;
    let (commit, date) = (bundled.dll_commit(), bundled.dll_commit_date());
    let used = ctx.resolve_spec(None)?;

    let ready = ctx.catalog.ready();
    let index = if ready {
        "index: ready"
    } else {
        "index: warming up (spec tools work; library tools use the bundled snapshot)"
    };
    let mut lines = vec![
        format!(
            "spec extracted from XXMI-Libs-Package {} ({date})",
            short(&commit)
        ),
        format!("default {}", used.line()),
        index.to_string(),
    ];
    lines.extend(repos.iter().flat_map(repo_lines));

    let mut spec = Map::new();
    spec.insert("dllCommit".into(), commit.into());
    spec.insert("dllCommitDate".into(), date.into());
    spec.insert("baselineCommit".into(), bundled.baseline_commit().into());
    let data = json!({
        "ready": ready,
        "spec": spec,
        "defaultSpec": {
            "source": if used.extracted { "extracted" } else { "bundled" },
            "dllSha": used.dll_sha,
            "tag": used.tag,
            "kind": used.kind,
        },
        "repos": repos,
    });
    Ok(ToolOutput {
        text: lines.join("\n"),
        data,
    })
}
