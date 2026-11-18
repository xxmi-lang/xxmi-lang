//! Which refs of which repos are indexed right now, and how tool inputs (`ref`, `repo`) map to
//! a checkout. Loaded from `state.json` once; the indexer is the only writer.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::repos::{RepoRole, TRACKED_REPOS, TrackedRepo, find_repo};
use crate::store::{Store, read_json};

pub const KIND_HEAD: &str = "head";

/// `kind` is head, release, prerelease or tag: a git tag without a GitHub release, or one GitHub
/// hasn't been asked about yet.
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub branch: VersionRef,
    /// Newest first.
    #[serde(default)]
    pub tags: Vec<VersionRef>,
    pub indexed_at: String,
}

#[derive(Deserialize)]
struct StateFile {
    version: u32,
    #[serde(default)]
    repos: HashMap<String, RepoState>,
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

pub struct Catalog {
    store: Store,
    repos: HashMap<&'static str, RepoState>,
}

impl Catalog {
    /// Loads `state.json`, dropping refs whose checkout vanished from the volume.
    pub async fn load(store: Store) -> Self {
        let mut catalog = Self {
            store,
            repos: HashMap::new(),
        };
        let Some(saved) = read_json::<StateFile>(catalog.store.state_file()).await else {
            return catalog;
        };
        if saved.version != 1 {
            return catalog;
        }

        for repo in &TRACKED_REPOS {
            let Some(mut entry) = saved.repos.get(repo.alias).cloned() else {
                continue;
            };
            if !catalog.store.has_checkout(repo, &entry.branch.sha) {
                continue;
            }
            entry.branch.kind = KIND_HEAD.to_string();
            entry
                .tags
                .retain(|t| catalog.store.has_checkout(repo, &t.sha));
            catalog.repos.insert(repo.alias, entry);
        }
        catalog
    }

    pub fn repo_state(&self, repo: &TrackedRepo) -> Option<&RepoState> {
        self.repos.get(repo.alias)
    }

    /// True once every tracked repo but the baseline has its default branch indexed: the baseline
    /// only backs DLL comparisons, so a failing one must not keep the server "warming up".
    pub fn ready(&self) -> bool {
        TRACKED_REPOS
            .iter()
            .all(|r| r.role == RepoRole::Baseline || self.repos.contains_key(r.alias))
    }

    pub fn is_ready(&self, repo: &TrackedRepo) -> bool {
        self.repos.contains_key(repo.alias)
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
    /// `latest-release` and `latest-prerelease` follow the releases; otherwise a tag or branch
    /// name, or a commit sha prefix (7+ characters) of an indexed ref.
    pub fn resolve(
        &self,
        repo: &'static TrackedRepo,
        wanted: Option<&str>,
    ) -> Result<ResolvedRef, CatalogError> {
        let Some(entry) = self.repo_state(repo) else {
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
