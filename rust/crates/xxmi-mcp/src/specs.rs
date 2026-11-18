//! The specs the server can answer with: the one bundled with this release (always there) and the
//! ones built per indexed DLL commit. `resolve` maps a `dll_ref` to one of them, falling back to
//! the bundled spec with a note while a version's own spec is missing.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use xxmi_core::lookup::Lookup;

use crate::bundled::{BundledSpec, meta_str};
use crate::catalog::{Catalog, CatalogError, ResolvedRef};
use crate::repos::dll_repo;

pub const DEFAULT_DLL_REF: &str = "latest-release";
pub const BUNDLED_REF: &str = "bundled";

/// Refs that mean "the newest of a kind": they fall back to the bundled spec when none exists.
const SYMBOLIC_REFS: [&str; 2] = ["latest-release", "latest-prerelease"];

/// A spec built from one DLL commit.
pub struct ExtractedSpec {
    pub lookup: &'static Lookup,
    pub doc: Arc<Value>,
}

/// The build result for one DLL commit.
pub enum SpecSlot {
    Pending,
    Failed(String),
    Ready(ExtractedSpec),
}

/// Where per-version specs come from. Nothing builds them until the indexer is ported, so the
/// server answers from the bundled spec.
pub trait VersionSpecs: Send + Sync {
    fn slot(&self, dll_sha: &str) -> SpecSlot;
}

pub struct NoVersionSpecs;

impl VersionSpecs for NoVersionSpecs {
    fn slot(&self, _dll_sha: &str) -> SpecSlot {
        SpecSlot::Pending
    }
}

pub struct SpecStatus {
    pub state: &'static str,
    pub error: Option<String>,
}

pub struct ResolvedSpec {
    pub lookup: &'static Lookup,
    pub doc: Arc<Value>,
    pub extracted: bool,
    /// What the caller asked for (the default when nothing was passed).
    pub dll_ref: String,
    /// The DLL commit the spec was extracted from.
    pub dll_sha: String,
    pub dll_commit_date: String,
    /// The indexed version (tag or default branch) of an extracted spec.
    pub name: Option<String>,
    pub tag: Option<String>,
    pub kind: Option<String>,
    /// Why the bundled spec answers instead of the requested one, as a clause.
    pub fallback: Option<String>,
    pub note: Option<String>,
}

pub fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

impl ResolvedSpec {
    /// The `spec` block of a tool's structured result.
    pub fn block(&self) -> Value {
        let mut block = Map::new();
        block.insert(
            "source".into(),
            if self.extracted {
                "extracted"
            } else {
                "bundled"
            }
            .into(),
        );
        block.insert("dllRef".into(), self.dll_ref.clone().into());
        block.insert("dllSha".into(), self.dll_sha.clone().into());
        block.insert("tag".into(), json!(self.tag));
        block.insert("kind".into(), json!(self.kind));
        block.insert("dllCommitDate".into(), self.dll_commit_date.clone().into());
        if let Some(note) = &self.note {
            block.insert("note".into(), note.clone().into());
        }
        Value::Object(block)
    }

    /// `spec: XXMI-Libs-Package v1.2.3 (release, a1b2c3d)` or `spec: bundled (b0bff3f); requested …`.
    pub fn line(&self) -> String {
        let sha = short(&self.dll_sha);
        if self.extracted {
            let name = self.name.as_deref().unwrap_or_default();
            let kind = self.kind.as_deref().unwrap_or_default();
            return format!("spec: XXMI-Libs-Package {name} ({kind}, {sha})");
        }
        match &self.fallback {
            Some(why) => format!("spec: bundled ({sha}); {why}"),
            None => format!("spec: bundled ({sha})"),
        }
    }
}

pub struct SpecRegistry {
    pub bundled: BundledSpec,
    versions: Box<dyn VersionSpecs>,
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl SpecRegistry {
    pub fn new(versions: Box<dyn VersionSpecs>) -> Self {
        Self {
            bundled: BundledSpec::load(),
            versions,
        }
    }

    pub fn status(&self, dll_sha: &str) -> SpecStatus {
        match self.versions.slot(dll_sha) {
            SpecSlot::Pending => SpecStatus {
                state: "pending",
                error: None,
            },
            SpecSlot::Ready(_) => SpecStatus {
                state: "ok",
                error: None,
            },
            SpecSlot::Failed(why) => SpecStatus {
                state: "failed",
                error: Some(why),
            },
        }
    }

    /// The spec for a `dll_ref`: unknown refs are an error, missing specs fall back to the
    /// bundled one.
    pub fn resolve(
        &self,
        catalog: &Catalog,
        requested: Option<&str>,
    ) -> Result<ResolvedSpec, CatalogError> {
        let dll_ref = requested
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .unwrap_or(DEFAULT_DLL_REF);
        if dll_ref.eq_ignore_ascii_case(BUNDLED_REF) {
            return Ok(self.bundled_result(dll_ref, None));
        }

        let repo = dll_repo();
        if !catalog.is_ready(repo) {
            let why = format!("requested {dll_ref} but the DLL index is still warming up");
            return Ok(self.bundled_result(dll_ref, Some(why)));
        }

        match catalog.resolve(repo, Some(dll_ref)) {
            Ok(target) => Ok(self.for_version(&target, dll_ref)),
            Err(_) if SYMBOLIC_REFS.contains(&dll_ref.to_lowercase().as_str()) => {
                let why = format!("requested {dll_ref} but no such version is indexed");
                Ok(self.bundled_result(dll_ref, Some(why)))
            }
            Err(CatalogError(message)) => Err(CatalogError(format!(
                "{message} Also valid: latest-release, latest-prerelease, head, {BUNDLED_REF}."
            ))),
        }
    }

    /// The spec of an already resolved DLL version.
    pub fn for_version(&self, target: &ResolvedRef, dll_ref: &str) -> ResolvedSpec {
        let label = format!("{} ({})", target.name, short(&target.sha));
        match self.versions.slot(&target.sha) {
            SpecSlot::Pending => self.bundled_result(
                dll_ref,
                Some(format!("requested {label} is still building")),
            ),
            SpecSlot::Failed(why) => self.bundled_result(
                dll_ref,
                Some(format!("requested {label} could not be built ({why})")),
            ),
            SpecSlot::Ready(spec) => ResolvedSpec {
                lookup: spec.lookup,
                dll_commit_date: meta_str(&spec.doc, &["meta", "dll", "commitDate"]),
                doc: spec.doc,
                extracted: true,
                dll_ref: dll_ref.to_string(),
                dll_sha: target.sha.clone(),
                name: Some(target.name.clone()),
                tag: (target.kind != "head").then(|| target.name.clone()),
                kind: Some(target.kind.clone()),
                fallback: None,
                note: None,
            },
        }
    }

    fn bundled_result(&self, dll_ref: &str, fallback: Option<String>) -> ResolvedSpec {
        let commit = self.bundled.dll_commit();
        let note = fallback.as_ref().map(|why| {
            format!(
                "{}; answering from the spec bundled with this server ({} {}).",
                capitalised(why),
                dll_repo().full_name(),
                short(&commit)
            )
        });
        ResolvedSpec {
            lookup: self.bundled.lookup,
            doc: self.bundled.doc.clone(),
            extracted: false,
            dll_ref: dll_ref.to_string(),
            dll_commit_date: self.bundled.dll_commit_date(),
            dll_sha: commit,
            name: None,
            tag: None,
            kind: None,
            fallback,
            note,
        }
    }
}
