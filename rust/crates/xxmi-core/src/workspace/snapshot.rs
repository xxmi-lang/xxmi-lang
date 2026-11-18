//! Library symbol snapshots (docs/01-language-model.md §4 "Bundled library snapshot"): the
//! sections, globals and namespaces of a package's libraries, so a mod linted on its own still
//! resolves `CommandList\ZZMI\...`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::SymbolKind;
use crate::paths::relative_path;
use crate::query::utf16_cmp;

use super::Workspace;
use super::symbols::global_variable_key;

#[derive(Debug, Deserialize, Serialize)]
pub struct SymbolSnapshot {
    pub version: u32,
    pub name: String,
    pub source: SnapshotSource,
    pub namespaces: Vec<String>,
    pub sections: Vec<SnapshotSection>,
    pub variables: Vec<SnapshotVariable>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SnapshotSource {
    pub repository: String,
    pub commit: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSection {
    pub kind: SymbolKind,
    pub qualified_name: String,
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SnapshotVariable {
    pub key: String,
    pub persist: bool,
    pub locked: bool,
    pub path: String,
    pub line: u32,
}

impl SymbolSnapshot {
    /// Symbols of every file the workspace loaded under `root`. Output is sorted, so the same
    /// package commit gives the same bytes.
    pub fn build(workspace: &Workspace, root: &Path, name: String, source: SnapshotSource) -> Self {
        let rel = |path: &Path| relative_path(root, path).replace(std::path::MAIN_SEPARATOR, "/");
        let mut sections = Vec::new();
        let mut variables = Vec::new();
        let mut namespaces: Vec<String> = Vec::new();

        for file in workspace.files() {
            if relative_path(root, &file.path).starts_with("..") {
                continue;
            }
            if !file.namespace.is_empty() && !namespaces.contains(&file.namespace) {
                namespaces.push(file.namespace.clone());
            }

            for section in &file.sections {
                let Some(kind) = section.kind else { continue };
                sections.push(SnapshotSection {
                    kind,
                    qualified_name: section.qualified_name.clone(),
                    path: rel(&file.path),
                    line: section.header.line + 1,
                });
            }
            for declaration in &file.variables {
                if declaration.scope != crate::model::Scope::Global {
                    continue;
                }
                variables.push(SnapshotVariable {
                    key: global_variable_key(&declaration.name, &file.namespace),
                    persist: declaration.persist,
                    locked: declaration.locked,
                    path: rel(&file.path),
                    line: file.range_of(declaration.span).start.line + 1,
                });
            }
        }

        namespaces.sort_by(|a, b| utf16_cmp(a, b));
        sections.sort_by(|a, b| {
            utf16_cmp(
                &a.qualified_name.to_lowercase(),
                &b.qualified_name.to_lowercase(),
            )
            .then_with(|| utf16_cmp(&a.path, &b.path))
        });
        variables
            .sort_by(|a, b| utf16_cmp(&a.key, &b.key).then_with(|| utf16_cmp(&a.path, &b.path)));

        Self {
            version: 1,
            name,
            source,
            namespaces,
            sections,
            variables,
        }
    }
}
