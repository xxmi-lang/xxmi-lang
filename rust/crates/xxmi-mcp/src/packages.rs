//! Loads indexed package checkouts through core (`Workspace`, `SymbolSnapshot`): the same code
//! builds the symbol index at indexing time and answers `library_*` and `lint_ini` later.
//!
//! A `Workspace` is not `Send`, so every load happens inside one blocking task and lives
//! only as long as the tool call that needs its docs.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

pub const DEFAULT_CACHED_SNAPSHOTS: usize = 64;

#[derive(Default)]
struct Lru {
    entries: HashMap<String, (Arc<SymbolSnapshot>, u64)>,
    clock: u64,
}

impl Lru {
    fn get(&mut self, key: &str) -> Option<Arc<SymbolSnapshot>> {
        self.clock += 1;
        let (snapshot, used) = self.entries.get_mut(key)?;
        *used = self.clock;
        Some(snapshot.clone())
    }

    fn insert(
        &mut self,
        key: String,
        snapshot: Arc<SymbolSnapshot>,
        max: usize,
    ) -> Arc<SymbolSnapshot> {
        self.clock += 1;
        let (kept, _) = self
            .entries
            .entry(key)
            .or_insert_with(|| (snapshot, self.clock));
        let kept = kept.clone();
        while self.entries.len() > max {
            let oldest = self.entries.iter().min_by_key(|(_, (_, used))| *used);
            let Some(oldest) = oldest.map(|(key, _)| key.clone()) else {
                break;
            };
            self.entries.remove(&oldest);
        }
        kept
    }
}

/// The snapshots of the package commits seen recently. A commit's snapshot never changes, so a
/// hit is always valid; the least recently used one goes first so the cache stays bounded however
/// many commits the indexer sees.
pub struct PackageCache {
    max_entries: usize,
    snapshots: Mutex<Lru>,
}

impl Default for PackageCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHED_SNAPSHOTS)
    }
}

impl PackageCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            snapshots: Mutex::new(Lru::default()),
        }
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The snapshot of `reference`, built from `loaded` when the caller already parsed it.
    pub fn snapshot(
        &self,
        reference: &ResolvedRef,
        loaded: Option<&LoadedIndex>,
    ) -> io::Result<Arc<SymbolSnapshot>> {
        let key = format!("{}@{}", reference.repo.alias, reference.sha);
        if let Some(known) = self.lock().get(&key) {
            return Ok(known);
        }

        let built = match loaded {
            Some(loaded) => build_snapshot(reference.repo, &reference.sha, loaded),
            None => build_snapshot(reference.repo, &reference.sha, &load_package(reference)?),
        };
        Ok(self.lock().insert(key, Arc::new(built), self.max_entries))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Lru> {
        self.snapshots.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(commit: &str) -> Arc<SymbolSnapshot> {
        Arc::new(SymbolSnapshot {
            version: 1,
            name: "zzmi".into(),
            source: SnapshotSource {
                repository: "https://github.com/x/y".into(),
                commit: commit.into(),
            },
            namespaces: Vec::new(),
            sections: Vec::new(),
            variables: Vec::new(),
        })
    }

    #[test]
    fn the_least_recently_used_snapshot_is_evicted() {
        let mut lru = Lru::default();
        lru.insert("a".into(), snapshot("a"), 2);
        lru.insert("b".into(), snapshot("b"), 2);
        lru.get("a");

        lru.insert("c".into(), snapshot("c"), 2);

        assert!(lru.get("a").is_some());
        assert!(lru.get("b").is_none());
        assert!(lru.get("c").is_some());
    }

    #[test]
    fn a_racing_insert_keeps_the_first_snapshot() {
        let mut lru = Lru::default();
        let first = lru.insert("a".into(), snapshot("first"), 2);

        let second = lru.insert("a".into(), snapshot("second"), 2);

        assert!(Arc::ptr_eq(&first, &second));
    }
}
