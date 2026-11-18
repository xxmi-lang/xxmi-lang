//! Library symbol snapshots (docs/01-language-model.md §4 "Bundled library snapshot"): the
//! sections, globals and namespaces of a package's libraries, so a mod linted on its own still
//! resolves `CommandList\ZZMI\...`. Only what resolution reads is deserialised.

use serde::Deserialize;

use crate::model::SymbolKind;

#[derive(Debug, Deserialize)]
pub struct SymbolSnapshot {
    pub namespaces: Vec<String>,
    pub sections: Vec<SnapshotSection>,
    pub variables: Vec<SnapshotVariable>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSection {
    pub kind: SymbolKind,
    pub qualified_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SnapshotVariable {
    pub key: String,
}
