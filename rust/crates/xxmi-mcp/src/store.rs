//! The data volume (docs/06-mcp-server.md "As built"), the same layout the TypeScript server uses:
//!
//!   <DATA_DIR>/repos/<owner>/<repo>/<sha>/      checkout of one commit (text files only)
//!   <DATA_DIR>/index/<owner>__<repo>/<sha>.json  library symbols of a package commit
//!   <DATA_DIR>/specs/<dllSha>.json               spec built from one DLL commit
//!   <DATA_DIR>/state.json                        which refs are indexed
//!   <DATA_DIR>/tmp/                              staging area
//!
//! This server only reads the volume; the indexer (stubbed, see indexer.rs) is its only writer.

use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::repos::TrackedRepo;

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
}

/// A JSON file parsed; a missing or unreadable file is `None`, as a half-written volume is no
/// reason to fail a request.
pub async fn read_json<T: DeserializeOwned + Send + 'static>(path: PathBuf) -> Option<T> {
    let bytes = tokio::fs::read(&path).await.ok()?;
    tokio::task::spawn_blocking(move || serde_json::from_slice(&bytes).ok())
        .await
        .ok()
        .flatten()
}
