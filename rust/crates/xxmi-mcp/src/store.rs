//! The data volume (docs/06-mcp-server.md "As built"), the same layout the TypeScript server uses:
//!
//!   <DATA_DIR>/repos/<owner>/<repo>/<sha>/      checkout of one commit (text files only)
//!   <DATA_DIR>/index/<owner>__<repo>/<sha>.json  library symbols of a package commit
//!   <DATA_DIR>/specs/<dllSha>.json               spec built from one DLL commit
//!   <DATA_DIR>/state.json                        which refs are indexed
//!   <DATA_DIR>/tmp/                              staging area
//!
//! Everything becomes visible by a rename,
//! so a crash never leaves a half-written checkout or state file behind. Only the indexer writes.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::repos::TrackedRepo;

static NEXT_NAME: AtomicU64 = AtomicU64::new(0);

/// A name no other call of this process, or of an earlier one, is likely to pick.
fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = NEXT_NAME.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{count:x}")
}

#[derive(Clone, Debug)]
pub struct Store {
    pub data_dir: PathBuf,
}

impl Store {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn checkout_dir(&self, repo: &TrackedRepo, sha: &str) -> PathBuf {
        self.data_dir
            .join("repos")
            .join(repo.owner)
            .join(repo.name)
            .join(sha)
    }

    pub fn index_file(&self, repo: &TrackedRepo, sha: &str) -> PathBuf {
        self.data_dir
            .join("index")
            .join(repo.dir_name())
            .join(format!("{sha}.json"))
    }

    pub fn spec_file(&self, dll_sha: &str) -> PathBuf {
        self.data_dir.join("specs").join(format!("{dll_sha}.json"))
    }

    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    pub fn has_checkout(&self, repo: &TrackedRepo, sha: &str) -> bool {
        self.checkout_dir(repo, sha).exists()
    }

    fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }

    /// Creates the staging area and clears the leftovers of an interrupted run.
    pub async fn init(&self) -> io::Result<()> {
        let tmp = self.tmp_dir();
        tokio::fs::create_dir_all(&tmp).await?;
        let mut entries = tokio::fs::read_dir(&tmp).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if entry.file_type().await?.is_dir() {
                tokio::fs::remove_dir_all(path).await?;
            } else {
                tokio::fs::remove_file(path).await?;
            }
        }
        Ok(())
    }

    pub fn new_staging_dir(&self, repo: &TrackedRepo) -> PathBuf {
        self.tmp_dir()
            .join(format!("{}-{}", repo.dir_name(), unique_suffix()))
    }

    /// Publishes a finished staging directory as the checkout of `sha`.
    pub async fn commit_checkout(
        &self,
        repo: &TrackedRepo,
        sha: &str,
        staging: &Path,
    ) -> io::Result<()> {
        let target = self.checkout_dir(repo, sha);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if target.exists() {
            tokio::fs::remove_dir_all(staging).await
        } else {
            tokio::fs::rename(staging, target).await
        }
    }

    pub async fn write_index<T: Serialize>(
        &self,
        repo: &TrackedRepo,
        sha: &str,
        value: &T,
    ) -> io::Result<()> {
        self.write_json_atomic(&self.index_file(repo, sha), value)
            .await
    }

    pub async fn write_state<T: Serialize>(&self, value: &T) -> io::Result<()> {
        self.write_json_atomic(&self.state_file(), value).await
    }

    pub async fn write_json_atomic<T: Serialize>(&self, path: &Path, value: &T) -> io::Result<()> {
        let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = self.tmp_dir().join(format!("{}.json", unique_suffix()));
        tokio::fs::create_dir_all(self.tmp_dir()).await?;
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, path).await
    }

    /// Removes checkouts and indexes of `repo` whose sha isn't in `keep`.
    pub async fn prune(&self, repo: &TrackedRepo, keep: &HashSet<String>) -> Vec<String> {
        let checkouts = self.data_dir.join("repos").join(repo.owner).join(repo.name);
        let mut removed = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(&checkouts).await else {
            return removed;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let sha = entry.file_name().to_string_lossy().into_owned();
            if keep.contains(&sha) {
                continue;
            }
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
            let _ = tokio::fs::remove_file(self.index_file(repo, &sha)).await;
            removed.push(sha);
        }
        removed
    }
}

/// The synchronous form, for code that already runs on a blocking thread.
pub fn read_json_blocking<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// A JSON file parsed;
/// a missing or unreadable file is `None`, as a half-written volume is no reason to fail a request.
pub async fn read_json<T: DeserializeOwned + Send + 'static>(path: PathBuf) -> Option<T> {
    let bytes = tokio::fs::read(&path).await.ok()?;
    tokio::task::spawn_blocking(move || serde_json::from_slice(&bytes).ok())
        .await
        .ok()
        .flatten()
}
