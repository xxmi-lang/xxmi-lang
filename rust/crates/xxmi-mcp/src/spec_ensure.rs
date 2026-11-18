//! Builds the specs the volume is missing, one at a time, for the indexer's pass.

use std::collections::HashSet;

use serde_json::json;

use crate::catalog::{Catalog, RepoState, VersionRef};
use crate::logger::{Logger, iso_timestamp};
use crate::repos::{TrackedRepo, baseline_repo, dll_repo};
use crate::spec_build::{BuildInput, SourceInput, SpecBuilder};
use crate::spec_store::{SpecHeader, StoredSpec, VolumeSpecs};

const MAX_WARNINGS: usize = 50;
const MAX_ERROR_CHARS: usize = 500;

pub struct EnsureDeps<'a> {
    pub specs: &'a VolumeSpecs,
    pub catalog: &'a Catalog,
    pub builder: &'a SpecBuilder,
    pub logger: Logger,
}

/// A commit can be the head and a tag at once; it needs one spec.
fn distinct_versions(state: &RepoState) -> Vec<VersionRef> {
    let mut seen = HashSet::new();
    std::iter::once(&state.branch)
        .chain(&state.tags)
        .filter(|v| seen.insert(v.sha.clone()))
        .cloned()
        .collect()
}

fn source_of(
    catalog: &Catalog,
    repo: &'static TrackedRepo,
    version: &VersionRef,
) -> Option<SourceInput> {
    let target = catalog.resolve(repo, Some(&version.sha)).ok()?;
    Some(SourceInput {
        dir: target.dir,
        commit: version.sha.clone(),
        commit_date: version.committed_at.clone().unwrap_or_default(),
        repository: format!("https://github.com/{}", repo.full_name()),
    })
}

/// Builds the specs of every indexed DLL version the volume has no current entry for.
pub async fn ensure_specs(deps: &EnsureDeps<'_>) -> Vec<String> {
    let (dll, baseline) = (dll_repo(), baseline_repo());
    let (Some(dll_state), Some(baseline_state)) = (
        deps.catalog.repo_state(dll),
        deps.catalog.repo_state(baseline),
    ) else {
        // Without the baseline the xxmi flags can't be set: try again on the next pass.
        return Vec::new();
    };
    let Some(baseline_input) = source_of(deps.catalog, baseline, &baseline_state.branch) else {
        return Vec::new();
    };

    let mut built = Vec::new();
    for version in distinct_versions(&dll_state) {
        if deps.specs.has_current(&version.sha) {
            continue;
        }
        let Some(dll_input) = source_of(deps.catalog, dll, &version) else {
            continue;
        };
        let input = BuildInput {
            dll: dll_input,
            baseline: baseline_input.clone(),
        };
        build_one(deps, input).await;
        built.push(version.sha);
    }
    built
}

/// Records the outcome either way: a failed build is not retried until its inputs change.
async fn build_one(deps: &EnsureDeps<'_>, input: BuildInput) {
    let started = std::time::Instant::now();
    let header = |status: &str| SpecHeader {
        dll_sha: input.dll.commit.clone(),
        baseline_sha: input.baseline.commit.clone(),
        builder: deps.specs.builder().to_string(),
        built_at: iso_timestamp(),
        status: status.to_string(),
        error: None,
        warnings: Vec::new(),
    };

    let stored = match deps.builder.build(input.clone()).await {
        Ok(built) => StoredSpec {
            header: SpecHeader {
                warnings: built.warnings.into_iter().take(MAX_WARNINGS).collect(),
                ..header("ok")
            },
            spec: Some(built.spec),
        },
        Err(error) => StoredSpec {
            header: SpecHeader {
                error: Some(error.chars().take(MAX_ERROR_CHARS).collect()),
                ..header("failed")
            },
            spec: None,
        },
    };

    let (status, error) = (stored.header.status.clone(), stored.header.error.clone());
    match deps.specs.record(stored).await {
        Ok(()) => deps.logger.info(
            "spec built",
            json!({
                "dll": input.dll.commit,
                "status": status,
                "ms": started.elapsed().as_millis() as u64,
                "error": error,
            }),
        ),
        Err(e) => deps.logger.warn(
            "spec not saved",
            json!({ "dll": input.dll.commit, "error": e.to_string() }),
        ),
    }
}
