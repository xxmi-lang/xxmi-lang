//! The bare repositories the server fetches into, one per tracked repo
//! (`<DATA_DIR>/git/<owner>/<repo>.git`). They only ever hold shallow fetches of the commits we
//! index, and are deleted and rebuilt when damaged or too big.

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;

use crate::exec::{Call, ExecError, GitCommand};
use crate::logger::Logger;
use crate::repos::{TrackedRepo, repo_url};

pub const LIST_TIMEOUT: Duration = Duration::from_secs(30);
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(180);

fn tree_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => tree_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

pub struct BareCaches {
    data_dir: PathBuf,
    git: GitCommand,
    logger: Logger,
    /// Repos whose cache was already rebuilt in the current pass: a second rebuild can't help.
    recreated: Mutex<HashSet<String>>,
}

impl BareCaches {
    pub fn new(data_dir: PathBuf, git: GitCommand, logger: Logger) -> Self {
        Self {
            data_dir,
            git,
            logger,
            recreated: Mutex::new(HashSet::new()),
        }
    }

    pub fn git(&self) -> &GitCommand {
        &self.git
    }

    pub fn dir(&self, repo: &TrackedRepo) -> PathBuf {
        self.data_dir
            .join("git")
            .join(repo.owner)
            .join(format!("{}.git", repo.name))
    }

    pub async fn in_repo(
        &self,
        repo: &TrackedRepo,
        args: &[&str],
        label: &str,
        timeout: Duration,
    ) -> Result<String, ExecError> {
        let dir = self.dir(repo);
        let call = Call {
            cwd: Some(&dir),
            timeout,
            label,
        };
        self.git.run(args, &call).await
    }

    /// Gives `repo` a fresh rebuild allowance; called when a poll pass starts on it.
    pub fn new_pass(&self, repo: &TrackedRepo) {
        self.recreated().remove(&repo.full_name());
    }

    fn recreated(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.recreated.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs `op` against a healthy cache; after a git failure that is not a timeout, rebuilds the
    /// cache and runs it once more. Network outages fail the same way twice and then propagate.
    pub async fn with_recovery<T, F, Fut>(&self, repo: &TrackedRepo, op: F) -> Result<T, String>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, RecoverableError>>,
    {
        self.ensure(repo).await.map_err(|e| e.to_string())?;
        match op().await {
            Ok(value) => Ok(value),
            Err(RecoverableError::Git(error))
                if !error.timed_out && !self.recreated().contains(&repo.full_name()) =>
            {
                self.recreate(repo, &error.message)
                    .await
                    .map_err(|e| e.to_string())?;
                op().await.map_err(|e| e.to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// Makes sure `sha` is in the cache, fetching it (or the ref it was listed under) if not.
    pub async fn ensure_commit(
        &self,
        repo: &TrackedRepo,
        sha: &str,
        ref_hint: Option<&str>,
    ) -> Result<(), ExecError> {
        if self.has_commit(repo, sha).await {
            return Ok(());
        }

        if let Err(error) = self.shallow_fetch(repo, sha).await {
            // Servers without protocol v2 may refuse a fetch by sha; the ref it was listed under
            // works.
            let Some(hint) = ref_hint else {
                return Err(error);
            };
            self.shallow_fetch(repo, hint).await?;
        }
        if !self.has_commit(repo, sha).await {
            let message = format!("{sha} is not on the remote any more");
            return Err(ExecError {
                message,
                timed_out: false,
            });
        }
        Ok(())
    }

    /// Drops the cache when it outgrew `max_bytes`; true if it did.
    pub async fn trim(&self, repo: &TrackedRepo, max_bytes: u64) -> bool {
        let dir = self.dir(repo);
        let size = tokio::task::spawn_blocking(move || tree_bytes(&dir))
            .await
            .unwrap_or(0);
        if size <= max_bytes {
            return false;
        }
        self.remove(repo).await;
        true
    }

    async fn ensure(&self, repo: &TrackedRepo) -> Result<(), ExecError> {
        if self.is_missing_or_empty(repo).await {
            self.create(repo).await
        } else if !self.healthy(repo).await? {
            self.recreate(repo, "failed the health check").await
        } else {
            Ok(())
        }
    }

    async fn is_missing_or_empty(&self, repo: &TrackedRepo) -> bool {
        match tokio::fs::read_dir(self.dir(repo)).await {
            Ok(mut entries) => matches!(entries.next_entry().await, Ok(None)),
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        }
    }

    async fn create(&self, repo: &TrackedRepo) -> Result<(), ExecError> {
        let dir = self.dir(repo);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ExecError {
                message: format!("git init failed: {e}"),
                timed_out: false,
            })?;

        let path = dir.to_string_lossy().into_owned();
        let call = Call {
            cwd: None,
            timeout: LIST_TIMEOUT,
            label: "git init",
        };
        self.git
            .run(&["init", "--bare", "--quiet", &path], &call)
            .await?;
        let url = repo_url(repo);
        self.in_repo(
            repo,
            &["remote", "add", "origin", &url],
            "git remote",
            LIST_TIMEOUT,
        )
        .await?;
        Ok(())
    }

    async fn remove(&self, repo: &TrackedRepo) {
        let _ = tokio::fs::remove_dir_all(self.dir(repo)).await;
    }

    async fn recreate(&self, repo: &TrackedRepo, reason: &str) -> Result<(), ExecError> {
        self.recreated().insert(repo.full_name());
        self.logger.warn(
            "git cache re-created",
            json!({ "repo": repo.full_name(), "reason": reason }),
        );
        self.remove(repo).await;
        self.create(repo).await
    }

    /// A bare repository whose `origin` is the expected URL; anything else is a half-made cache.
    /// The URL is read from the config, not `git remote get-url`, which applies `insteadOf`.
    async fn healthy(&self, repo: &TrackedRepo) -> Result<bool, ExecError> {
        let checked = async {
            let bare = self
                .in_repo(
                    repo,
                    &["rev-parse", "--is-bare-repository"],
                    "git rev-parse",
                    LIST_TIMEOUT,
                )
                .await?;
            let url = self
                .in_repo(
                    repo,
                    &["config", "--get", "remote.origin.url"],
                    "git config",
                    LIST_TIMEOUT,
                )
                .await?;
            Ok::<bool, ExecError>(bare.trim() == "true" && url.trim() == repo_url(repo))
        };
        match checked.await {
            Err(error) if error.timed_out => Err(error),
            Err(_) => Ok(false),
            Ok(healthy) => Ok(healthy),
        }
    }

    async fn has_commit(&self, repo: &TrackedRepo, sha: &str) -> bool {
        let spec = format!("{sha}^{{commit}}");
        self.in_repo(
            repo,
            &["cat-file", "-e", "--end-of-options", &spec],
            "git cat-file",
            LIST_TIMEOUT,
        )
        .await
        .is_ok()
    }

    async fn shallow_fetch(&self, repo: &TrackedRepo, what: &str) -> Result<(), ExecError> {
        let args = [
            "-c",
            "protocol.version=2",
            "fetch",
            "--depth",
            "1",
            "--no-tags",
            "--end-of-options",
            "origin",
            what,
        ];
        self.in_repo(repo, &args, "git fetch", TRANSFER_TIMEOUT)
            .await
            .map(drop)
    }
}

/// What `with_recovery` can tell apart: a git failure is worth a rebuilt cache, the rest is not.
pub enum RecoverableError {
    Git(ExecError),
    Other(String),
}

impl std::fmt::Display for RecoverableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(error) => error.fmt(f),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl From<ExecError> for RecoverableError {
    fn from(error: ExecError) -> Self {
        Self::Git(error)
    }
}
