//! The data core ships with (port of packages/core/src/defaults.ts): the merged spec and the
//! library snapshots, both embedded at build time by build.rs.

use std::sync::OnceLock;

use crate::lookup::Lookup;
use crate::spec::Spec;
use crate::workspace::snapshot::SymbolSnapshot;

const BUNDLED_SPEC: &str = include_str!(env!("XXMI_SPEC_JSON"));
const BUNDLED_SNAPSHOTS: [&str; 1] = [include_str!(env!("XXMI_ZZMI_SNAPSHOT_JSON"))];

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
        BUNDLED_SNAPSHOTS
            .iter()
            .map(|json| serde_json::from_str(json).expect("the bundled snapshot parses"))
            .collect()
    })
}
