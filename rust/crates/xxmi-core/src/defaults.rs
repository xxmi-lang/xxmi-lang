//! The data core ships with: the merged spec and the library snapshots,
//! both embedded at build time by build.rs.

use std::sync::OnceLock;

use crate::lookup::Lookup;
use crate::profile::PROFILES;
use crate::spec::Spec;
use crate::workspace::snapshot::SymbolSnapshot;

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
