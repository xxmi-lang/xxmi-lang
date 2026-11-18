//! Git-backed upstream access: `ls-remote` for refs and `git archive` for files, never a
//! checkout. The bare caches it fetches into are in bare.rs.

mod bare;
mod extract;
mod ls_remote;
pub mod releases;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tokio_util::io::SyncIoBridge;

pub use bare::{LIST_TIMEOUT, TRANSFER_TIMEOUT};
pub use extract::keep_path;
pub use ls_remote::{RemoteRefs, RemoteTag, parse_ls_remote};

use crate::exec::{Call, ExecError, GitCommand};
use crate::logger::Logger;
use crate::repos::{TrackedRepo, repo_url};
use bare::{BareCaches, RecoverableError};
use extract::extract_tar;

const GIT_EXIT_GRACE: Duration = Duration::from_secs(2);

pub struct CommitInfo {
    /// ISO committer date.
    pub date: String,
    pub subject: String,
}

pub struct GitSources {
    bare: BareCaches,
    /// Which ref pointed at a commit at the last `list_refs`: the fallback when a sha can't be
    /// fetched.
    ref_hints: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl GitSources {
    pub fn new(data_dir: PathBuf, git: GitCommand, logger: Logger) -> Self {
        Self {
            bare: BareCaches::new(data_dir, git, logger),
            ref_hints: Mutex::new(HashMap::new()),
        }
    }

    fn remember_refs(&self, repo: &TrackedRepo, refs: &RemoteRefs) {
        let mut hints: HashMap<String, String> = refs
            .tags
            .iter()
            .map(|t| (t.sha.clone(), format!("refs/tags/{}", t.name)))
            .collect();
        hints.insert(
            refs.head.clone(),
            format!("refs/heads/{}", refs.default_branch),
        );
        self.ref_hints
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(repo.full_name(), hints);
    }

    fn hint(&self, repo: &TrackedRepo, sha: &str) -> Option<String> {
        let hints = self.ref_hints.lock().unwrap_or_else(|e| e.into_inner());
        hints.get(&repo.full_name())?.get(sha).cloned()
    }

    pub async fn list_refs(&self, repo: &TrackedRepo) -> Result<RemoteRefs, String> {
        self.bare.new_pass(repo);
        let url = repo_url(repo);
        let args = [
            "-c",
            "protocol.version=2",
            "ls-remote",
            "--symref",
            "--end-of-options",
            &url,
            "HEAD",
            "refs/heads/*",
            "refs/tags/*",
        ];
        let call = Call {
            cwd: None,
            timeout: LIST_TIMEOUT,
            label: "git ls-remote",
        };
        let output = self
            .bare
            .git()
            .run(&args, &call)
            .await
            .map_err(|e| e.to_string())?;

        let refs = parse_ls_remote(&output)?;
        self.remember_refs(repo, &refs);
        Ok(refs)
    }

    /// Reads a commit that `materialize` fetched before.
    pub async fn commit_info(&self, repo: &TrackedRepo, sha: &str) -> Result<CommitInfo, String> {
        let output = self
            .bare
            .in_repo(
                repo,
                &["log", "-1", "--format=%cI%n%s", "--end-of-options", sha],
                "git log",
                LIST_TIMEOUT,
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut lines = output.split('\n');
        Ok(CommitInfo {
            date: lines.next().unwrap_or_default().to_string(),
            subject: lines.next().unwrap_or_default().to_string(),
        })
    }

    /// Writes the text files of `sha` into `dest`; returns the bytes written.
    pub async fn materialize(
        &self,
        repo: &TrackedRepo,
        sha: &str,
        dest: &Path,
    ) -> Result<u64, String> {
        let hint = self.hint(repo, sha);
        self.bare
            .with_recovery(repo, || async {
                self.bare.ensure_commit(repo, sha, hint.as_deref()).await?;
                self.extract_archive(repo, sha, dest).await
            })
            .await
    }

    /// Drops the local cache of `repo` when it outgrew `max_bytes`; true if it did.
    pub async fn trim_cache(&self, repo: &TrackedRepo, max_bytes: u64) -> bool {
        self.bare.trim(repo, max_bytes).await
    }

    /// Streams `git archive` of `sha` into the extractor; a git failure explains a cut tar best.
    async fn extract_archive(
        &self,
        repo: &TrackedRepo,
        sha: &str,
        dest: &Path,
    ) -> Result<u64, RecoverableError> {
        let dir = self.bare.dir(repo);
        let call = Call {
            cwd: Some(&dir),
            timeout: TRANSFER_TIMEOUT,
            label: "git archive",
        };
        let args = ["archive", "--format=tar", "--end-of-options", sha];
        let mut archive = self.bare.git().stream(&args, &call)?;

        let stdout = SyncIoBridge::new(archive.take_stdout());
        let (dest, role) = (dest.to_path_buf(), repo.role);
        let extraction = tokio::task::spawn_blocking(move || extract_tar(stdout, &dest, role));

        let both = async {
            let extracted = extraction.await;
            let failure = match &extracted {
                Ok(Ok(_)) => archive.finish().await.err(),
                _ => archive.explain(GIT_EXIT_GRACE).await,
            };
            (extracted, failure)
        };
        let Ok((extracted, failure)) = tokio::time::timeout(TRANSFER_TIMEOUT, both).await else {
            return Err(archive_timeout(&call).into());
        };

        if let Some(error) = failure {
            return Err(error.into());
        }
        match extracted {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(error)) => Err(RecoverableError::Other(error.to_string())),
            Err(error) => Err(RecoverableError::Other(error.to_string())),
        }
    }
}

fn archive_timeout(call: &Call<'_>) -> ExecError {
    ExecError {
        message: format!(
            "{} failed: timed out after {} ms",
            call.label,
            call.timeout.as_millis()
        ),
        timed_out: true,
    }
}
