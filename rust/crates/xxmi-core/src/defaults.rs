//! The data core ships with: the merged spec and the library snapshots,
//! both embedded at build time by build.rs.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::lookup::Lookup;
use crate::model::SymbolKind;
use crate::profile::PROFILES;
use crate::spec::Spec;
use crate::workspace::snapshot::SymbolSnapshot;
use crate::workspace::symbols::explicit_namespace;

const BUNDLED_SPEC: &str = include_str!(env!("XXMI_SPEC_JSON"));

/// The merged spec as JSON text, for callers that serve or re-read the document itself.
pub fn bundled_spec_json() -> &'static str {
    BUNDLED_SPEC
}

pub fn bundled_lookup() -> &'static Lookup {
    static LOOKUP: OnceLock<Lookup> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        let spec = Spec::from_json(BUNDLED_SPEC).expect("the bundled spec parses");
        Lookup::new(spec.leak())
    })
}

pub fn bundled_snapshots() -> &'static [SymbolSnapshot] {
    static SNAPSHOTS: OnceLock<Vec<SymbolSnapshot>> = OnceLock::new();
    SNAPSHOTS.get_or_init(|| {
        PROFILES
            .iter()
            .filter_map(|profile| profile.snapshot)
            .map(|json| serde_json::from_str(json).expect("the bundled snapshot parses"))
            .collect()
    })
}

/// Whether some bundled library declares a section of this kind under this name in the empty
/// namespace (`CommandListSkinTexture`), compared case-insensitively like the DLL.
pub fn is_library_global(kind: SymbolKind, name: &str) -> bool {
    static GLOBALS: OnceLock<HashSet<(SymbolKind, String)>> = OnceLock::new();
    let globals = GLOBALS.get_or_init(|| {
        let lookup = bundled_lookup();
        bundled_snapshots()
            .iter()
            .flat_map(|snapshot| &snapshot.sections)
            .filter(|section| explicit_namespace(&section.qualified_name, lookup).is_none())
            .map(|section| (section.kind, section.qualified_name.to_lowercase()))
            .collect()
    });
    globals.contains(&(kind, name.to_lowercase()))
}
