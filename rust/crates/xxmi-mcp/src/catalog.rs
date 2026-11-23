//! Which refs of which repos are indexed right now, and how tool inputs
//! (`ref`, `repo`) map to a checkout. Loaded from `state.json` once;
//! the indexer is the only writer.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::repos::{RepoRole, TRACKED_REPOS, TrackedRepo, find_repo};
use crate::store::{Store, read_json};
use crate::upstream::releases::ReleasesState;

pub const KIND_HEAD: &str = "head";

/// `kind` is head, release, prerelease or tag:
/// a git tag without a GitHub release, or one GitHub hasn't been asked about yet.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRef {
    pub name: String,
    pub sha: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

/// Files from before release tracking have no `kind` on their tags.
fn default_kind() -> String {
    "tag".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub branch: VersionRef,
    /// Newest first.
    #[serde(default)]
    pub tags: Vec<VersionRef>,
    pub indexed_at: String,
}

/// `state.json`: what is indexed, and what GitHub said about each repo's tags.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateFile {
    pub version: u32,
    #[serde(default)]
    pub repos: BTreeMap<String, RepoState>,
    /// Kept until the tag set changes; files from before release tracking have none.
    #[serde(default)]
    pub releases: BTreeMap<String, ReleasesState>,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: 1,
            repos: BTreeMap::new(),
            releases: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedRef {
    pub repo: &'static TrackedRepo,
    pub name: String,
    pub sha: String,
    pub dir: PathBuf,
    pub is_default: bool,
    pub kind: String,
}

/// A failure the caller can act on: an unknown repo or ref, an index that is still warming up.
#[derive(Debug)]
pub struct CatalogError(pub String);

/// The newest release; repos without GitHub releases fall back to their newest tag.
pub fn latest_release(entry: &RepoState) -> Option<&VersionRef> {
    let release = entry.tags.iter().find(|t| t.kind == "release");
    let has_releases = entry
        .tags
        .iter()
        .any(|t| t.kind == "release" || t.kind == "prerelease");
    release.or_else(|| {
        if has_releases {
            None
        } else {
            entry.tags.iter().find(|t| t.kind == "tag")
        }
    })
}

/// The newest pre-release; the indexed ones are always newer than the newest release.
pub fn latest_prerelease(entry: &RepoState) -> Option<&VersionRef> {
    entry.tags.iter().find(|t| t.kind == "prerelease")
}

fn find_version<'a>(entry: &'a RepoState, wanted: &str) -> Option<&'a VersionRef> {
    let lower = wanted.to_lowercase();
    match lower.as_str() {
        "" | "latest" | "head" => return Some(&entry.branch),
        "latest-release" => return latest_release(entry),
        "latest-prerelease" => return latest_prerelease(entry).or_else(|| latest_release(entry)),
        _ => {}
    }

    let mut all = std::iter::once(&entry.branch).chain(&entry.tags);
    let by_name = all.clone().find(|r| r.name.to_lowercase() == lower);
    by_name.or_else(|| {
        if lower.len() >= 7 {
            all.find(|r| r.sha.starts_with(&lower))
        } else {
            None
        }
    })
}

/// Files from before release tracking have no `kind` on their tags;
/// refs whose checkout is gone are dropped.
fn present_on_volume(store: &Store, mut saved: StateFile) -> StateFile {
    let mut state = StateFile::default();
    for repo in &TRACKED_REPOS {
        let Some(mut entry) = saved.repos.remove(repo.alias) else {
            continue;
        };
        if !store.has_checkout(repo, &entry.branch.sha) {
            continue;
        }
        entry.branch.kind = KIND_HEAD.to_string();
        entry.tags.retain(|t| store.has_checkout(repo, &t.sha));
        state.repos.insert(repo.alias.to_string(), entry);
        if let Some(releases) = saved.releases.remove(repo.alias) {
            state.releases.insert(repo.alias.to_string(), releases);
        }
    }
    state
}

/// The indexer is the only writer; readers see a new state as soon as it is published.
pub struct Catalog {
    store: Store,
    state: RwLock<Arc<StateFile>>,
}

impl Catalog {
    /// Loads `state.json`, dropping refs whose checkout vanished from the volume.
    pub async fn load(store: Store) -> Self {
        let saved = read_json::<StateFile>(store.state_file()).await;
        let state = saved
            .filter(|saved| saved.version == 1)
            .map(|saved| present_on_volume(&store, saved))
            .unwrap_or_default();
        Self {
            store,
            state: RwLock::new(Arc::new(state)),
        }
    }

    pub fn snapshot(&self) -> Arc<StateFile> {
        self.state.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Publishes a new state (the indexer calls this after the files are in place).
    pub async fn publish(&self, next: StateFile) -> std::io::Result<()> {
        self.store.write_state(&next).await?;
        *self.state.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(next);
        Ok(())
    }

    pub fn repo_state(&self, repo: &TrackedRepo) -> Option<RepoState> {
        self.snapshot().repos.get(repo.alias).cloned()
    }

    /// True once every tracked repo but the baseline has its default branch indexed:
    /// the baseline only backs DLL comparisons,
    /// so a failing one must not keep the server "warming up".
    pub fn ready(&self) -> bool {
        let state = self.snapshot();
        TRACKED_REPOS
            .iter()
            .all(|r| r.role == RepoRole::Baseline || state.repos.contains_key(r.alias))
    }

    pub fn is_ready(&self, repo: &TrackedRepo) -> bool {
        self.snapshot().repos.contains_key(repo.alias)
    }

    /// Shas that must stay on disk.
    pub fn kept_shas(&self, repo: &TrackedRepo) -> HashSet<String> {
        self.repo_state(repo)
            .map(|entry| {
                std::iter::once(entry.branch.sha)
                    .chain(entry.tags.into_iter().map(|t| t.sha))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `owner/name` or alias to a tracked repo; the error lists the allowed names.
    pub fn repo_by_name(&self, query: &str) -> Result<&'static TrackedRepo, CatalogError> {
        find_repo(query).ok_or_else(|| {
            let allowed: Vec<String> = TRACKED_REPOS
                .iter()
                .map(|r| format!("{} ({})", r.alias, r.full_name()))
                .collect();
            CatalogError(format!(
                "Unknown repo \"{query}\". Tracked repos: {}.",
                allowed.join(", ")
            ))
        })
    }

    /// The checkout for `wanted`: omitted, `latest` or `head` is the default branch;
    /// `latest-release` and `latest-prerelease` follow the releases;
    /// otherwise a tag or branch name, or a commit sha prefix (7+ characters) of an indexed ref.
    pub fn resolve(
        &self,
        repo: &'static TrackedRepo,
        wanted: Option<&str>,
    ) -> Result<ResolvedRef, CatalogError> {
        let state = self.snapshot();
        let Some(entry) = state.repos.get(repo.alias) else {
            return Err(CatalogError(format!(
                "{} is not indexed yet (the server is warming up). Try again in a minute.",
                repo.full_name()
            )));
        };

        let wanted = wanted.map_or("", str::trim);
        let Some(found) = find_version(entry, wanted) else {
            let names: Vec<&str> = std::iter::once(&entry.branch)
                .chain(&entry.tags)
                .map(|r| r.name.as_str())
                .collect();
            return Err(CatalogError(format!(
                "ref \"{wanted}\" is not indexed for {}. Indexed: {}.",
                repo.full_name(),
                names.join(", ")
            )));
        };

        Ok(ResolvedRef {
            repo,
            name: found.name.clone(),
            sha: found.sha.clone(),
            dir: self.store.checkout_dir(repo, &found.sha),
            is_default: std::ptr::eq(found, &entry.branch),
            kind: found.kind.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(name: &str, sha: &str, kind: &str) -> VersionRef {
        VersionRef {
            name: name.into(),
            sha: sha.into(),
            kind: kind.into(),
            tag: None,
            published_at: None,
            committed_at: None,
            subject: None,
        }
    }

    fn entry() -> RepoState {
        RepoState {
            branch: version("main", &"a".repeat(40), "head"),
            tags: vec![
                version("v2-beta", &"b".repeat(40), "prerelease"),
                version("v1", &"c".repeat(40), "release"),
            ],
            indexed_at: String::new(),
        }
    }

    #[test]
    fn symbolic_refs_follow_the_release_kinds() {
        let entry = entry();
        assert_eq!(find_version(&entry, "latest-release").unwrap().name, "v1");
        assert_eq!(
            find_version(&entry, "latest-prerelease").unwrap().name,
            "v2-beta"
        );
        assert_eq!(find_version(&entry, "").unwrap().name, "main");
    }

    #[test]
    fn names_are_caseless_and_shas_need_seven_characters() {
        let entry = entry();
        assert_eq!(find_version(&entry, "V1").unwrap().name, "v1");
        assert_eq!(find_version(&entry, &"c".repeat(7)).unwrap().name, "v1");
        assert!(find_version(&entry, "cccccc").is_none());
    }
}
