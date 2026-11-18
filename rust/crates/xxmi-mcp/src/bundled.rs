//! The spec and library snapshots built into this release, served until a version's own spec
//! exists.

use std::sync::{Arc, LazyLock};

use serde_json::Value;
use xxmi_core::defaults::{bundled_lookup, bundled_snapshots, bundled_spec_json};
use xxmi_core::lookup::Lookup;
use xxmi_core::workspace::snapshot::SymbolSnapshot;

pub struct BundledSpec {
    pub lookup: &'static Lookup,
    /// The spec file as JSON: the source of its `meta` and of the `xxmi://spec` documents.
    pub doc: Arc<Value>,
}

/// `doc[path...]` as a string, or empty when the spec lacks it.
pub fn meta_str(doc: &Value, path: &[&str]) -> String {
    path.iter()
        .try_fold(doc, |at, key| at.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

static DOC: LazyLock<Arc<Value>> = LazyLock::new(|| {
    Arc::new(serde_json::from_str(bundled_spec_json()).expect("the bundled spec is valid JSON"))
});

impl BundledSpec {
    pub fn load() -> Self {
        Self {
            lookup: bundled_lookup(),
            doc: DOC.clone(),
        }
    }

    pub fn dll_commit(&self) -> String {
        meta_str(&self.doc, &["meta", "dll", "commit"])
    }

    pub fn dll_commit_date(&self) -> String {
        meta_str(&self.doc, &["meta", "dll", "commitDate"])
    }

    pub fn baseline_commit(&self) -> String {
        meta_str(&self.doc, &["meta", "baseline", "commit"])
    }
}

/// The snapshot bundled for a game profile, by name.
pub fn bundled_snapshot(game: &str) -> Option<&'static SymbolSnapshot> {
    bundled_snapshots().iter().find(|s| s.name == game)
}
