//! Polls the tracked repos and keeps the volume current (docs/06-mcp-server.md "Architecture").
//! Per pass: `git ls-remote` each repo, work out the wanted versions,
//! fetch and extract each missing commit, build the symbol index for packages,
//! and only then publish the new state.
//! Readers keep the old checkout until the swap, and a failed fetch leaves everything as it was.
//! Specs per DLL version are built after publishing, so they never delay readiness.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;

use crate::catalog::{Catalog, KIND_HEAD, RepoState, StateFile, VersionRef};
use crate::logger::{Logger, iso_timestamp};
use crate::packages::index_checkout;
use crate::repos::{RepoRole, TRACKED_REPOS, TrackedRepo};
use crate::selection::{SelectionLimits, select_versions, tracks_tags};
use crate::spec_build::SpecBuilder;
use crate::spec_ensure::{EnsureDeps, ensure_specs};
use crate::spec_store::VolumeSpecs;
use crate::store::Store;
use crate::upstream::releases::{RefreshOptions, ReleaseLookup, ReleasesState, refresh_releases};
use crate::upstream::{GitSources, RemoteRefs};

/// What one indexing pass did, as `POST /admin/refresh?wait=1` reports it.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PassResult {
    /// `owner/name@ref` for everything newly published.
    pub published: Vec<String>,
    pub errors: Vec<String>,
}

pub trait Indexer: Send + Sync {
    fn refresh(&self) -> Pin<Box<dyn Future<Output = PassResult> + Send + '_>>;
}

const RETRY_WHILE_NOT_READY: Duration = Duration::from_secs(60);

/// Poll interval, or a quick retry while the first index is still missing (GitHub hiccup).
pub fn next_delay(result: &PassResult, ready: bool, poll_interval_min: u64) -> Duration {
    let interval = Duration::from_secs(poll_interval_min * 60);
    if !result.errors.is_empty() && !ready {
        RETRY_WHILE_NOT_READY.min(interval)
    } else {
        interval
    }
}

pub struct IndexerOptions {
    pub store: Store,
    pub catalog: Arc<Catalog>,
    pub sources: GitSources,
    pub logger: Logger,
    pub limits: SelectionLimits,
    pub git_cache_max_mb: u64,
    pub poll_interval_min: u64,
    pub specs: Arc<VolumeSpecs>,
    pub spec_builder: SpecBuilder,
}

struct Head {
    refs: RemoteRefs,
    version: VersionRef,
}

/// What to write into the next `state.json` for one repo; unset parts stay as they are.
struct StatePatch {
    branch: VersionRef,
    tags: Option<Vec<VersionRef>>,
    releases: Option<ReleasesState>,
}

#[derive(Default)]
struct Finished {
    last: PassResult,
}

pub struct PollingIndexer<R: ReleaseLookup> {
    o: IndexerOptions,
    releases: R,
    /// Held for the length of a pass; what it guards is the result later callers share.
    running: Mutex<Finished>,
    passes: AtomicU64,
    force_releases: AtomicBool,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn full_names(repo: &TrackedRepo, version: &VersionRef) -> String {
    format!("{}@{}", repo.full_name(), version.name)
}

impl<R: ReleaseLookup + 'static> PollingIndexer<R> {
    pub fn new(options: IndexerOptions, releases: R) -> Self {
        Self {
            o: options,
            releases,
            running: Mutex::new(Finished::default()),
            passes: AtomicU64::new(0),
            force_releases: AtomicBool::new(false),
        }
    }

    /// Starts the poll loop on the runtime; a non-positive interval disables polling.
    pub fn start(self: &Arc<Self>) {
        if self.o.poll_interval_min == 0 {
            return;
        }
        let indexer = self.clone();
        tokio::spawn(async move {
            loop {
                let result = indexer.run_once().await;
                let delay = next_delay(
                    &result,
                    indexer.o.catalog.ready(),
                    indexer.o.poll_interval_min,
                );
                tokio::time::sleep(delay).await;
            }
        });
    }

    /// One polling pass. Calls that wait for a running pass share its result.
    pub async fn run_once(&self) -> PassResult {
        let seen = self.passes.load(Ordering::Acquire);
        let mut running = self.running.lock().await;
        if self.passes.load(Ordering::Acquire) != seen {
            return running.last.clone();
        }
        self.run_pass(&mut running).await
    }

    /// A pass that also asks GitHub about releases again, even if the tags didn't change.
    pub async fn force_refresh(&self) -> PassResult {
        self.force_releases.store(true, Ordering::Release);
        let mut running = self.running.lock().await;
        if !self.force_releases.load(Ordering::Acquire) {
            return running.last.clone();
        }
        self.run_pass(&mut running).await
    }

    async fn run_pass(&self, running: &mut Finished) -> PassResult {
        let result = self.pass().await;
        running.last = result.clone();
        self.passes.fetch_add(1, Ordering::Release);
        result
    }

    async fn pass(&self) -> PassResult {
        let mut result = PassResult::default();
        let started = Instant::now();
        let force = self.force_releases.swap(false, Ordering::AcqRel);
        let mut heads: HashMap<&'static str, Head> = HashMap::new();
        // The commits readers may still be using: they survive one more pass.
        let grace = self.kept_now();

        for repo in &TRACKED_REPOS {
            if let Err(error) = self.index_head(repo, &mut heads, &mut result).await {
                self.step_failed(repo, &mut result, error);
            }
        }

        // Tags come second so the server is ready as soon as every default branch is in.
        for repo in &TRACKED_REPOS {
            let Some(head) = heads.get(repo.alias) else {
                continue;
            };
            if !tracks_tags(repo.role) {
                continue;
            }
            if let Err(error) = self.index_tags(repo, head, &mut result, force).await {
                self.step_failed(repo, &mut result, error);
            }
        }

        self.build_specs().await;
        self.prune(&grace).await;
        self.o.logger.info(
            "index pass finished",
            json!({
                "ms": started.elapsed().as_millis() as u64,
                "published": result.published,
                "errors": result.errors.len(),
            }),
        );
        result
    }

    fn step_failed(&self, repo: &TrackedRepo, result: &mut PassResult, error: String) {
        result.errors.push(format!("{}: {error}", repo.full_name()));
        self.o.logger.warn(
            "index step failed",
            json!({ "repo": repo.full_name(), "error": error }),
        );
    }

    /// A failing build is recorded, never an error of the pass, and nothing here gates readiness.
    async fn build_specs(&self) {
        let deps = EnsureDeps {
            specs: &self.o.specs,
            catalog: &self.o.catalog,
            builder: &self.o.spec_builder,
            logger: self.o.logger,
        };
        ensure_specs(&deps).await;
    }

    async fn index_head(
        &self,
        repo: &'static TrackedRepo,
        heads: &mut HashMap<&'static str, Head>,
        result: &mut PassResult,
    ) -> Result<(), String> {
        let refs = self.o.sources.list_refs(repo).await?;
        let wanted = VersionRef {
            name: refs.default_branch.clone(),
            sha: refs.head.clone(),
            kind: KIND_HEAD.to_string(),
            tag: None,
            published_at: None,
            committed_at: None,
            subject: None,
        };

        // Nothing is published for a head that couldn't be fetched: readers keep the old one.
        let Some(version) = self.ensure(repo, wanted, result).await else {
            return Ok(());
        };
        let patch = StatePatch {
            branch: version.clone(),
            tags: None,
            releases: None,
        };
        heads.insert(repo.alias, Head { refs, version });
        self.publish_state(repo, patch).await
    }

    async fn index_tags(
        &self,
        repo: &'static TrackedRepo,
        head: &Head,
        result: &mut PassResult,
        force: bool,
    ) -> Result<(), String> {
        let tag_names: Vec<String> = head.refs.tags.iter().map(|t| t.name.clone()).collect();
        let previous = self.o.catalog.snapshot().releases.get(repo.alias).cloned();
        let options = RefreshOptions {
            logger: &self.o.logger,
            now: now_ms(),
            force,
        };
        let releases = refresh_releases(
            &self.releases,
            repo,
            &tag_names,
            previous.as_ref(),
            &options,
        )
        .await;

        let wanted = select_versions(repo.role, &head.refs.tags, Some(&releases), &self.o.limits);
        let mut present = Vec::new();
        for version in wanted {
            if let Some(ensured) = self.ensure(repo, version, result).await {
                present.push(ensured);
            }
        }
        let patch = StatePatch {
            branch: head.version.clone(),
            tags: Some(present),
            releases: Some(releases),
        };
        self.publish_state(repo, patch).await
    }

    /// Makes sure `version` is on disk; returns it with its commit info, or `None` on failure.
    async fn ensure(
        &self,
        repo: &'static TrackedRepo,
        version: VersionRef,
        result: &mut PassResult,
    ) -> Option<VersionRef> {
        if self.o.store.has_checkout(repo, &version.sha) {
            return Some(self.with_known_commit_info(repo, version));
        }

        match self.fetch_and_index(repo, &version).await {
            Ok((committed_at, subject)) => {
                result.published.push(full_names(repo, &version));
                Some(VersionRef {
                    committed_at: Some(committed_at),
                    subject: Some(subject),
                    ..version
                })
            }
            Err(error) => {
                result
                    .errors
                    .push(format!("{}: {error}", full_names(repo, &version)));
                self.o.logger.warn(
                    "fetch failed",
                    json!({ "repo": repo.full_name(), "ref": version.name, "error": error }),
                );
                None
            }
        }
    }

    async fn fetch_and_index(
        &self,
        repo: &'static TrackedRepo,
        version: &VersionRef,
    ) -> Result<(String, String), String> {
        let started = Instant::now();
        let staging = self.o.store.new_staging_dir(repo);
        let outcome = self.stage(repo, version, &staging).await;
        let _ = tokio::fs::remove_dir_all(&staging).await;

        let (bytes, commit) = outcome?;
        self.o.logger.info(
            "indexed",
            json!({
                "repo": repo.full_name(),
                "ref": version.name,
                "sha": version.sha,
                "bytes": bytes,
                "ms": started.elapsed().as_millis() as u64,
            }),
        );
        Ok((commit.date, commit.subject))
    }

    async fn stage(
        &self,
        repo: &'static TrackedRepo,
        version: &VersionRef,
        staging: &std::path::Path,
    ) -> Result<(u64, crate::upstream::CommitInfo), String> {
        let sha = &version.sha;
        let bytes = self.o.sources.materialize(repo, sha, staging).await?;
        let commit = self.o.sources.commit_info(repo, sha).await?;
        if repo.role == RepoRole::Package {
            self.write_package_index(repo, sha, staging).await?;
        }
        self.o
            .store
            .commit_checkout(repo, sha, staging)
            .await
            .map_err(|e| e.to_string())?;
        Ok((bytes, commit))
    }

    /// A `Workspace` is not `Send`: it is built, snapshotted and dropped on one blocking thread.
    async fn write_package_index(
        &self,
        repo: &'static TrackedRepo,
        sha: &str,
        checkout: &std::path::Path,
    ) -> Result<(), String> {
        let (sha_owned, dir) = (sha.to_string(), checkout.to_path_buf());
        let snapshot = tokio::task::spawn_blocking(move || index_checkout(repo, &sha_owned, &dir))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        self.o
            .store
            .write_index(repo, sha, &snapshot)
            .await
            .map_err(|e| e.to_string())
    }

    /// Commit details of a version already published, so unchanged commits aren't re-read.
    fn with_known_commit_info(&self, repo: &TrackedRepo, version: VersionRef) -> VersionRef {
        let Some(entry) = self.o.catalog.repo_state(repo) else {
            return version;
        };
        let known = std::iter::once(&entry.branch)
            .chain(&entry.tags)
            .find(|v| v.sha == version.sha);
        VersionRef {
            committed_at: known.and_then(|v| v.committed_at.clone()),
            subject: known.and_then(|v| v.subject.clone()),
            ..version
        }
    }

    /// Swaps the catalog to the new refs, keeping what the patch leaves out.
    async fn publish_state(&self, repo: &TrackedRepo, patch: StatePatch) -> Result<(), String> {
        let current = self.o.catalog.snapshot();
        let previous = current.repos.get(repo.alias);
        let indexed_at = match previous {
            Some(p) if p.branch.sha == patch.branch.sha => p.indexed_at.clone(),
            _ => iso_timestamp(),
        };
        let entry = RepoState {
            tags: patch
                .tags
                .or_else(|| previous.map(|p| p.tags.clone()))
                .unwrap_or_default(),
            branch: patch.branch,
            indexed_at,
        };

        let mut next: StateFile = (*current).clone();
        next.repos.insert(repo.alias.to_string(), entry);
        if let Some(releases) = patch.releases {
            next.releases.insert(repo.alias.to_string(), releases);
        }
        self.o
            .catalog
            .publish(next)
            .await
            .map_err(|e| e.to_string())
    }

    fn kept_now(&self) -> HashMap<&'static str, HashSet<String>> {
        TRACKED_REPOS
            .iter()
            .map(|r| (r.alias, self.o.catalog.kept_shas(r)))
            .collect()
    }

    async fn prune(&self, grace: &HashMap<&'static str, HashSet<String>>) {
        for repo in &TRACKED_REPOS {
            let mut keep = self.o.catalog.kept_shas(repo);
            keep.extend(grace.get(repo.alias).into_iter().flatten().cloned());
            let removed = self.o.store.prune(repo, &keep).await;
            if !removed.is_empty() {
                self.o.logger.info(
                    "pruned",
                    json!({ "repo": repo.full_name(), "removed": removed }),
                );
            }
            self.trim_git_cache(repo).await;
        }
        self.prune_specs().await;
    }

    async fn prune_specs(&self) {
        let keep = self.o.catalog.kept_shas(crate::repos::dll_repo());
        let removed = self.o.specs.retain(&keep).await;
        if !removed.is_empty() {
            self.o
                .logger
                .info("pruned specs", json!({ "removed": removed }));
        }
    }

    /// The bare clone only grows; past the limit it is dropped and the next fetch rebuilds it.
    async fn trim_git_cache(&self, repo: &TrackedRepo) {
        let max_bytes = self.o.git_cache_max_mb * 1024 * 1024;
        if self.o.sources.trim_cache(repo, max_bytes).await {
            self.o
                .logger
                .info("git cache dropped", json!({ "repo": repo.full_name() }));
        }
    }
}

impl<R: ReleaseLookup + 'static> Indexer for PollingIndexer<R> {
    fn refresh(&self) -> Pin<Box<dyn Future<Output = PassResult> + Send + '_>> {
        Box::pin(self.force_refresh())
    }
}
