//! Built specs on the volume: `<DATA_DIR>/specs/<dllSha>.json`, one per indexed DLL commit,
//! in the shape the TypeScript server writes. A stored spec is served only
//! when it was built from the same DLL and baseline commits by this builder,
//! and is rebuilt otherwise.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use xxmi_core::lookup::Lookup;
use xxmi_core::spec::Spec;

use crate::catalog::Catalog;
use crate::repos::{baseline_repo, is_sha};
use crate::specs::{ExtractedSpec, SpecSlot, VersionSpecs};
use crate::store::{Store, read_json_blocking};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecHeader {
    pub dll_sha: String,
    pub baseline_sha: String,
    /// `builder_id()` of the server that built it.
    pub builder: String,
    pub built_at: String,
    /// `ok` or `failed`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StoredSpec {
    #[serde(flatten)]
    pub header: SpecHeader,
    /// Present when `status` is `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<Value>,
}

impl SpecHeader {
    fn is_current(&self, dll_sha: &str, baseline_sha: &str, builder: &str) -> bool {
        self.dll_sha == dll_sha && self.baseline_sha == baseline_sha && self.builder == builder
    }
}

/// Leaking a spec is the price of the `&'static` model references core hands out,
/// so a spec that is loaded again
/// (after an eviction, or by a second reader) must reuse its first lookup:
/// the leak is then one per build, not one per load.
type BuildKey = (String, String, String, String);

static LOOKUPS: LazyLock<Mutex<HashMap<BuildKey, &'static Lookup>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn build_key(header: &SpecHeader) -> BuildKey {
    (
        header.dll_sha.clone(),
        header.baseline_sha.clone(),
        header.builder.clone(),
        header.built_at.clone(),
    )
}

fn leaked_lookup(header: &SpecHeader, spec: Spec) -> &'static Lookup {
    let mut known = LOOKUPS.lock().unwrap_or_else(|e| e.into_inner());
    known
        .entry(build_key(header))
        .or_insert_with(|| Box::leak(Box::new(Lookup::new(spec.leak()))))
}

fn extracted(header: &SpecHeader, doc: Value) -> Option<ExtractedSpec> {
    let spec: Spec = serde_json::from_value(doc.clone()).ok()?;
    Some(ExtractedSpec {
        lookup: leaked_lookup(header, spec),
        doc: Arc::new(doc),
    })
}

fn slot_of(stored: StoredSpec) -> Option<SpecSlot> {
    if stored.header.status == "failed" {
        let why = stored
            .header
            .error
            .unwrap_or_else(|| "unknown error".into());
        return Some(SpecSlot::Failed(why));
    }
    extracted(&stored.header, stored.spec?).map(SpecSlot::Ready)
}

/// Reads the spec file of `dll_sha` when it was built from these inputs. Takes no lock:
/// it does file I/O and parsing.
fn load_current(store: &Store, dll_sha: &str, baseline_sha: &str, builder: &str) -> Option<Cached> {
    let stored = read_json_blocking::<StoredSpec>(&store.spec_file(dll_sha))?;
    if !stored.header.is_current(dll_sha, baseline_sha, builder) {
        return None;
    }
    let header = stored.header.clone();
    let slot = slot_of(stored)?;
    Some(Cached { header, slot })
}

struct Cached {
    header: SpecHeader,
    slot: SpecSlot,
}

/// The specs on the volume, loaded on first use.
pub struct VolumeSpecs {
    store: Store,
    catalog: Arc<Catalog>,
    builder: String,
    slots: Mutex<HashMap<String, Cached>>,
}

impl VolumeSpecs {
    pub fn new(store: Store, catalog: Arc<Catalog>, builder: String) -> Self {
        Self {
            store,
            catalog,
            builder,
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub fn builder(&self) -> &str {
        &self.builder
    }

    /// The baseline commit stored specs must have been built against; unknown until it is indexed.
    pub fn baseline_sha(&self) -> Option<String> {
        Some(self.catalog.repo_state(baseline_repo())?.branch.sha)
    }

    fn dir(&self) -> PathBuf {
        self.store.data_dir.join("specs")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Cached>> {
        self.slots.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether a current build result (spec or failure) exists for `dll_sha`.
    /// Called from the indexer's async pass, so a cache miss reads the file on a blocking thread.
    pub async fn has_current(&self, dll_sha: &str) -> bool {
        let Some(baseline_sha) = self.baseline_sha() else {
            return false;
        };
        if self.cached(dll_sha, &baseline_sha).is_some() {
            return true;
        }

        let (store, builder) = (self.store.clone(), self.builder.clone());
        let (sha, baseline) = (dll_sha.to_string(), baseline_sha);
        let loaded =
            tokio::task::spawn_blocking(move || load_current(&store, &sha, &baseline, &builder))
                .await;
        match loaded {
            Ok(Some(cached)) => {
                self.lock().insert(dll_sha.to_string(), cached);
                true
            }
            _ => false,
        }
    }

    /// The cached slot, dropping an entry built from inputs that have since changed.
    fn cached(&self, dll_sha: &str, baseline_sha: &str) -> Option<SpecSlot> {
        let mut slots = self.lock();
        let cached = slots.get(dll_sha)?;
        if cached
            .header
            .is_current(dll_sha, baseline_sha, &self.builder)
        {
            return Some(cached.slot.clone());
        }
        slots.remove(dll_sha);
        None
    }

    /// Runs on a blocking tool thread, so a cache miss may read the file there.
    fn slot_of_sha(&self, dll_sha: &str) -> SpecSlot {
        let Some(baseline_sha) = self.baseline_sha() else {
            return SpecSlot::Pending;
        };
        if let Some(slot) = self.cached(dll_sha, &baseline_sha) {
            return slot;
        }

        let Some(cached) = load_current(&self.store, dll_sha, &baseline_sha, &self.builder) else {
            return SpecSlot::Pending;
        };
        let slot = cached.slot.clone();
        self.lock().insert(dll_sha.to_string(), cached);
        slot
    }

    /// Saves a build result and serves it from now on.
    pub async fn record(&self, stored: StoredSpec) -> std::io::Result<()> {
        self.store
            .write_json_atomic(&self.store.spec_file(&stored.header.dll_sha), &stored)
            .await?;
        let (sha, header) = (stored.header.dll_sha.clone(), stored.header.clone());
        if let Some(slot) = slot_of(stored) {
            self.lock().insert(sha, Cached { header, slot });
        }
        Ok(())
    }

    /// Drops the specs of DLL commits that are no longer indexed, from memory and from the volume;
    /// returns their shas.
    pub async fn retain(&self, keep: &HashSet<String>) -> Vec<String> {
        self.lock().retain(|sha, _| keep.contains(sha));
        let mut removed = Vec::new();
        let Ok(mut entries) = tokio::fs::read_dir(self.dir()).await else {
            return removed;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(sha) = name.strip_suffix(".json").filter(|s| is_sha(s)) else {
                continue;
            };
            if keep.contains(sha) {
                continue;
            }
            let _ = tokio::fs::remove_file(entry.path()).await;
            removed.push(sha.to_string());
        }
        removed
    }
}

impl VersionSpecs for Arc<VolumeSpecs> {
    fn slot(&self, dll_sha: &str) -> SpecSlot {
        self.slot_of_sha(dll_sha)
    }
}

#[cfg(test)]
mod tests {
    use xxmi_core::defaults::bundled_spec_json;

    use super::*;

    fn header(built_at: &str) -> SpecHeader {
        SpecHeader {
            dll_sha: "a".repeat(40),
            baseline_sha: "b".repeat(40),
            builder: "test".into(),
            built_at: built_at.into(),
            status: "ok".into(),
            error: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn loading_the_same_build_twice_leaks_one_lookup() {
        let load = |header: &SpecHeader| {
            leaked_lookup(header, Spec::from_json(bundled_spec_json()).unwrap())
        };

        let first = load(&header("2026-01-01"));
        let again = load(&header("2026-01-01"));
        let rebuilt = load(&header("2026-01-02"));

        assert!(std::ptr::eq(first, again));
        assert!(!std::ptr::eq(first, rebuilt));
    }
}
