//! Built specs on the volume: `<DATA_DIR>/specs/<dllSha>.json`, one per indexed DLL commit, in the
//! shape the TypeScript server writes. A stored spec is served only when it was built from the
//! same DLL and baseline commits by this builder, and is rebuilt otherwise.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use xxmi_core::lookup::Lookup;
use xxmi_core::spec::Spec;

use crate::catalog::Catalog;
use crate::repos::baseline_repo;
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

/// The lookup of a stored spec; leaked, as every spec model is (see `Spec::leak`).
fn extracted(doc: Value) -> Option<ExtractedSpec> {
    let spec: Spec = serde_json::from_value(doc.clone()).ok()?;
    let lookup: &'static Lookup = Box::leak(Box::new(Lookup::new(spec.leak())));
    Some(ExtractedSpec {
        lookup,
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
    extracted(stored.spec?).map(SpecSlot::Ready)
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

    /// The baseline commit stored specs must have been built against; unknown until it is
    /// indexed.
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
    pub fn has_current(&self, dll_sha: &str) -> bool {
        !matches!(self.slot_of_sha(dll_sha), SpecSlot::Pending)
    }

    fn slot_of_sha(&self, dll_sha: &str) -> SpecSlot {
        let Some(baseline_sha) = self.baseline_sha() else {
            return SpecSlot::Pending;
        };
        let mut slots = self.lock();
        if let Some(cached) = slots.get(dll_sha) {
            if cached
                .header
                .is_current(dll_sha, &baseline_sha, &self.builder)
            {
                return cached.slot.clone();
            }
            slots.remove(dll_sha);
        }

        let path = self.store.spec_file(dll_sha);
        let Some(stored) = read_json_blocking::<StoredSpec>(&path) else {
            return SpecSlot::Pending;
        };
        if !stored
            .header
            .is_current(dll_sha, &baseline_sha, &self.builder)
        {
            return SpecSlot::Pending;
        }
        let header = stored.header.clone();
        let Some(slot) = slot_of(stored) else {
            return SpecSlot::Pending;
        };
        slots.insert(
            dll_sha.to_string(),
            Cached {
                header,
                slot: slot.clone(),
            },
        );
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

    /// Drops the specs of DLL commits that are no longer indexed, from memory and from the
    /// volume; returns their shas.
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

fn is_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl VersionSpecs for Arc<VolumeSpecs> {
    fn slot(&self, dll_sha: &str) -> SpecSlot {
        self.slot_of_sha(dll_sha)
    }
}
