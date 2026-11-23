//! What the references of a workspace resolve to, computed once per symbol table (the `usage`
//! cache of packages/core/src/lint/rules/references.ts): XM204 and XM205 read it.

use std::collections::HashSet;

use crate::model::SymbolKind;
use crate::model::mesh::binds_core_mesh_slot;

use super::Workspace;
use super::symbols::ResolvedVariable;

pub struct Usage {
    /// Lower-cased qualified names of sections some reference resolved to.
    pub sections: HashSet<String>,
    /// The lower-cased qualified names among them that a `vb0`-`vb2` or `ib` line binds.
    pub mesh_bound: HashSet<String>,
    /// Keys of global variables some reference resolved to.
    pub globals: HashSet<String>,
    /// Local declarations, by file path and declaration offset, some reference resolved to.
    pub locals: HashSet<(usize, usize)>,
}

/// Resolves every reference in the workspace once, recording what is used.
pub(super) fn compute(workspace: &Workspace) -> Usage {
    let symbols = workspace.symbols();
    let mut usage = Usage {
        sections: HashSet::new(),
        mesh_bound: HashSet::new(),
        globals: HashSet::new(),
        locals: HashSet::new(),
    };

    for (id, file) in workspace.files().iter().enumerate() {
        for reference in &file.references {
            if reference.kind == SymbolKind::Variable {
                match symbols.resolve_variable(reference, file) {
                    Some(ResolvedVariable::Local(start)) => {
                        usage.locals.insert((id, start));
                    }
                    Some(ResolvedVariable::Global(key)) => {
                        usage.globals.insert(key);
                    }
                    None => {}
                }
            } else if let Some(name) =
                symbols.resolve_section(reference.kind, &reference.text, &file.namespace)
            {
                let name = name.to_lowercase();
                if reference.kind == SymbolKind::Resource && binds_core_mesh_slot(file, reference) {
                    usage.mesh_bound.insert(name.clone());
                }
                usage.sections.insert(name);
            }
        }
    }
    usage
}
