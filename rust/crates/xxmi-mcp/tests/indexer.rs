//! The indexer against local git remotes: `insteadOf` points the GitHub URLs at them, so a pass
//! runs the real `ls-remote`, `fetch` and `archive` without a network.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use xxmi_mcp::catalog::Catalog;
use xxmi_mcp::exec::GitCommand;
use xxmi_mcp::indexer::{IndexerOptions, PollingIndexer};
use xxmi_mcp::logger::Logger;
use xxmi_mcp::repos::{TRACKED_REPOS, TrackedRepo, dll_repo};
use xxmi_mcp::selection::SelectionLimits;
use xxmi_mcp::spec_build::{SpecBuilder, builder_id};
use xxmi_mcp::spec_store::VolumeSpecs;
use xxmi_mcp::store::Store;
use xxmi_mcp::upstream::GitSources;
use xxmi_mcp::upstream::releases::{ReleaseInfo, ReleaseLookup, ReleaseMeta};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn remote_path(root: &Path, repo: &TrackedRepo) -> PathBuf {
    root.join(repo.owner).join(format!("{}.git", repo.name))
}

/// A bare remote with one commit on `main` tagged `v1.0`.
fn make_remote(root: &Path, repo: &TrackedRepo) -> PathBuf {
    let work = root.join("work").join(repo.name);
    let content = work.join(repo.package_dir.unwrap_or_default());
    std::fs::create_dir_all(&content).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    std::fs::write(content.join("d3dx.ini"), "[Constants]\n").unwrap();
    commit_all(&work, "first");
    git(&work, &["tag", "v1.0"]);

    let bare = remote_path(root, repo);
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    git(
        &work,
        &["clone", "-q", "--bare", ".", bare.to_str().unwrap()],
    );
    work
}

fn commit_all(work: &Path, message: &str) -> String {
    git(work, &["add", "-A"]);
    git(work, &["commit", "-q", "-m", message]);
    git(work, &["rev-parse", "HEAD"])
}

struct OneRelease;

impl ReleaseLookup for OneRelease {
    async fn releases(&self, _repo: &'static TrackedRepo) -> Result<Vec<ReleaseInfo>, String> {
        Ok(vec![ReleaseInfo {
            tag: "v1.0".into(),
            meta: ReleaseMeta {
                name: Some("One".into()),
                prerelease: false,
                draft: false,
                published_at: Some("2026-01-01T00:00:00Z".into()),
            },
        }])
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    root: PathBuf,
    works: Vec<PathBuf>,
    catalog: Arc<Catalog>,
    specs: Arc<VolumeSpecs>,
    indexer: PollingIndexer<OneRelease>,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("remotes");
    let works = TRACKED_REPOS
        .iter()
        .map(|r| make_remote(&root, r))
        .collect();

    let store = Store::new(dir.path().join("data"));
    let volume = store.clone();
    store.init().await.unwrap();
    let catalog = Arc::new(Catalog::load(store.clone()).await);
    let specs = Arc::new(VolumeSpecs::new(
        store.clone(),
        catalog.clone(),
        builder_id(),
    ));
    let rewrite = format!("url.{}/.insteadOf=https://github.com/", root.display());
    let git = GitCommand::new("git", &["-c", &rewrite]);
    let logger = Logger { quiet: true };
    let options = IndexerOptions {
        sources: GitSources::new(store.data_dir.clone(), git, logger),
        store,
        catalog: catalog.clone(),
        logger,
        limits: SelectionLimits {
            releases: 3,
            prereleases: 1,
        },
        git_cache_max_mb: 1024,
        poll_interval_min: 0,
        specs: specs.clone(),
        spec_builder: SpecBuilder::with(|_| Err("stub".into()), Duration::from_secs(5)),
    };
    let indexer = PollingIndexer::new(options, OneRelease);
    Fixture {
        _dir: dir,
        store: volume,
        root,
        works,
        catalog,
        specs,
        indexer,
    }
}

#[tokio::test]
async fn a_pass_indexes_heads_and_tags_then_idles() {
    let f = fixture().await;

    let first = f.indexer.run_once().await;

    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(f.catalog.ready());
    assert!(
        first
            .published
            .contains(&"leotorrez/ZZMI-Package@main".to_string())
    );
    let dll = f.catalog.repo_state(dll_repo()).unwrap();
    assert_eq!(dll.tags.len(), 1);
    assert_eq!(dll.tags[0].kind, "release");
    assert_eq!(dll.tags[0].tag.as_deref(), Some("v1.0"));
    assert!(
        f.specs.has_current(&dll.branch.sha),
        "the stubbed build is recorded as failed"
    );

    let second = f.indexer.run_once().await;
    assert!(second.published.is_empty(), "{:?}", second.published);
}

async fn push_commit(f: &Fixture, name: &str) -> String {
    std::fs::write(f.works[0].join(name), "x").unwrap();
    let sha = commit_all(&f.works[0], name);
    let remote = remote_path(&f.root, dll_repo());
    git(
        &f.works[0],
        &["push", "-q", remote.to_str().unwrap(), "main"],
    );
    sha
}

#[tokio::test]
async fn a_new_commit_is_published_and_the_old_one_pruned_a_pass_later() {
    let f = fixture().await;
    f.indexer.run_once().await;

    let middle = push_commit(&f, "second").await;
    let moved = f.indexer.run_once().await;
    assert_eq!(f.catalog.repo_state(dll_repo()).unwrap().branch.sha, middle);
    assert!(
        moved
            .published
            .contains(&"SpectrumQT/XXMI-Libs-Package@main".to_string())
    );
    assert!(f.store.has_checkout(dll_repo(), &middle));

    let last = push_commit(&f, "third").await;
    f.indexer.run_once().await;
    assert!(
        f.store.has_checkout(dll_repo(), &middle),
        "readers may still use it for one pass"
    );
    f.indexer.run_once().await;

    assert!(!f.store.has_checkout(dll_repo(), &middle));
    assert!(f.store.has_checkout(dll_repo(), &last));
}

#[tokio::test]
async fn an_unreachable_remote_is_an_error_not_a_crash() {
    let f = fixture().await;
    std::fs::remove_dir_all(remote_path(&f.root, dll_repo())).unwrap();

    let result = f.indexer.run_once().await;

    assert!(
        result
            .errors
            .iter()
            .any(|e| e.starts_with("SpectrumQT/XXMI-Libs-Package"))
    );
    assert!(!f.catalog.ready());
}
