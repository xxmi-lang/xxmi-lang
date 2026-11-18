//! Loads indexed package checkouts through core (`Workspace`, `SymbolSnapshot`): the same code
//! builds the symbol index at indexing time and answers `library_*` and `lint_ini` later.
//!
//! A `Workspace` is not `Send`, so every load happens inside one blocking task and lives
//! only as long as the tool call that needs its docs.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use xxmi_core::defaults::bundled_lookup;
use xxmi_core::query::library::LoadedPackage;
use xxmi_core::workspace::Workspace;
use xxmi_core::workspace::snapshot::{SnapshotSource, SymbolSnapshot};

use crate::catalog::ResolvedRef;
use crate::repos::TrackedRepo;

pub struct LoadedIndex {
    pub workspace: Workspace,
    pub root: PathBuf,
}

impl LoadedIndex {
    pub fn core(&self) -> LoadedPackage<'_> {
        LoadedPackage {
            workspace: &self.workspace,
            root: &self.root,
        }
    }
}

/// Parses the package of `reference`. The lookup is the bundled spec's: a package index must not
/// change with the DLL versions indexed.
pub fn load_package(reference: &ResolvedRef) -> io::Result<LoadedIndex> {
    load_checkout(reference.repo, &reference.dir)
}

fn load_checkout(repo: &TrackedRepo, checkout: &Path) -> io::Result<LoadedIndex> {
    let root = checkout.join(repo.package_dir.unwrap_or("."));
    let mut workspace = Workspace::new(bundled_lookup(), &[]);
    workspace.load_package(&root)?;
    Ok(LoadedIndex { workspace, root })
}

fn build_snapshot(repo: &TrackedRepo, sha: &str, loaded: &LoadedIndex) -> SymbolSnapshot {
    let name = repo
        .game
        .map_or_else(|| repo.name.to_lowercase(), String::from);
    let source = SnapshotSource {
        repository: format!("https://github.com/{}", repo.full_name()),
        commit: sha.to_string(),
    };
    SymbolSnapshot::build(&loaded.workspace, &loaded.root, name, source)
}

/// The symbol snapshot of a package checkout that isn't published yet (the indexer's staging
/// directory).
pub fn index_checkout(
    repo: &TrackedRepo,
    sha: &str,
    checkout: &Path,
) -> io::Result<SymbolSnapshot> {
    Ok(build_snapshot(repo, sha, &load_checkout(repo, checkout)?))
}

/// Snapshots of the package commits seen so far. The linter wants them for the life of the
/// process, and a commit's snapshot never changes, so each is leaked once.
#[derive(Default)]
pub struct PackageCache {
    snapshots: Mutex<HashMap<String, &'static SymbolSnapshot>>,
}

impl PackageCache {
    /// The snapshot of `reference`, built from `loaded` when the caller already parsed it.
    pub fn snapshot(
        &self,
        reference: &ResolvedRef,
        loaded: Option<&LoadedIndex>,
    ) -> io::Result<&'static SymbolSnapshot> {
        let key = format!("{}@{}", reference.repo.alias, reference.sha);
        if let Some(known) = self.lock().get(&key) {
            return Ok(known);
        }

        let built = match loaded {
            Some(loaded) => build_snapshot(reference.repo, &reference.sha, loaded),
            None => build_snapshot(reference.repo, &reference.sha, &load_package(reference)?),
        };
        Ok(self
            .lock()
            .entry(key)
            .or_insert_with(|| Box::leak(Box::new(built))))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, &'static SymbolSnapshot>> {
        self.snapshots.lock().unwrap_or_else(|e| e.into_inner())
    }
}
