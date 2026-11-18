//! Workspace-wide symbols and DLL-order name resolution (port of packages/core/src/workspace/symbols.ts,
//! docs/01-language-model.md §3 rules 1-7).
//!
//! The DLL stores a prefixed section from a namespaced file as `Prefix\namespace\Local` and a
//! global declared in namespace N as `$\n\name`. A reference first tries the name qualified with the
//! referencing file's namespace, then the name as written. All comparisons are case-insensitive.

use std::collections::{HashMap, HashSet};

use crate::lookup::Lookup;
use crate::model::{IniFile, Reference, Scope, SymbolKind};

use super::snapshot::SymbolSnapshot;

pub struct SectionSymbol {
    pub kind: SymbolKind,
    /// As the DLL stores it, e.g. `CommandList\ZZMI\SetTextures`.
    pub qualified_name: String,
}

/// What a variable reference resolved to; all rules need is which declaration was used.
#[derive(Debug, PartialEq, Eq)]
pub enum ResolvedVariable {
    /// A local of the referencing section, by the byte offset of its declaration.
    Local(usize),
    /// A global, by its stored key.
    Global(String),
}

/// The key the DLL stores a global under: lower-cased, `$\namespace\name` inside a namespace.
pub fn global_variable_key(name: &str, namespace: &str) -> String {
    let lower = name.to_lowercase();
    if namespace.is_empty() {
        return lower;
    }
    let bare = lower.strip_prefix('$').unwrap_or(&lower);
    format!("$\\{}\\{bare}", namespace.to_lowercase())
}

pub struct SymbolTable {
    lookup: &'static Lookup,
    sections: HashMap<String, Vec<SectionSymbol>>,
    variables: HashSet<String>,
    namespaces: HashSet<String>,
}

impl SymbolTable {
    pub fn new(lookup: &'static Lookup) -> Self {
        Self {
            lookup,
            sections: HashMap::new(),
            variables: HashSet::new(),
            namespaces: HashSet::new(),
        }
    }

    pub fn add_file(&mut self, file: &IniFile) {
        if !file.namespace.is_empty() {
            self.namespaces.insert(file.namespace.to_lowercase());
        }

        for section in &file.sections {
            if let Some(kind) = section.kind {
                self.add_section(kind, section.qualified_name.clone());
            }
        }
        for declaration in file.variables.iter().filter(|v| v.scope == Scope::Global) {
            self.variables
                .insert(global_variable_key(&declaration.name, &file.namespace));
        }
    }

    pub fn add_snapshot(&mut self, snapshot: &SymbolSnapshot) {
        for namespace in &snapshot.namespaces {
            self.namespaces.insert(namespace.to_lowercase());
        }
        for section in &snapshot.sections {
            self.add_section(section.kind, section.qualified_name.clone());
        }
        for variable in &snapshot.variables {
            self.variables.insert(variable.key.clone());
        }
    }

    /// Whether a loaded file or snapshot declares this namespace.
    pub fn has_namespace(&self, namespace: &str) -> bool {
        self.namespaces.contains(&namespace.to_lowercase())
    }

    pub fn all_sections(&self) -> impl Iterator<Item = &SectionSymbol> {
        self.sections.values().flatten()
    }

    /// Resolves a section reference made from `namespace`. The DLL's own built-in sections resolve
    /// to a symbol too.
    pub fn resolve_section(&self, kind: SymbolKind, text: &str, namespace: &str) -> Option<String> {
        for candidate in self.section_candidates(text, namespace) {
            let found = self
                .sections
                .get(&candidate.to_lowercase())
                .and_then(|list| list.iter().find(|s| s.kind == kind));
            if let Some(found) = found {
                return Some(found.qualified_name.clone());
            }
        }
        self.lookup
            .is_builtin_section(text)
            .then(|| text.to_string())
    }

    /// Candidate stored names for a section reference, in DLL lookup order.
    fn section_candidates(&self, text: &str, namespace: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        if let Some(prefix) = self.lookup.section_prefix(text)
            && !namespace.is_empty()
        {
            let local = text.get(prefix.len()..).unwrap_or_default();
            candidates.push(format!("{prefix}\\{namespace}\\{local}"));
        }
        candidates.push(text.to_string());
        candidates
    }

    /// Resolves `$name` / `$\ns\name` used in `file`. Locals declared earlier in the same section
    /// win (the DLL checks the command list's scope first), then globals in DLL order.
    pub fn resolve_variable(
        &self,
        reference: &Reference,
        file: &IniFile,
    ) -> Option<ResolvedVariable> {
        let lower = reference.text.to_lowercase();
        let local = file.variables.iter().find(|v| {
            v.scope == Scope::Local
                && v.section == reference.section
                && v.span.start <= reference.span.start
                && v.name.to_lowercase() == lower
        });
        if let Some(local) = local {
            return Some(ResolvedVariable::Local(local.span.start));
        }

        let mut candidates = Vec::new();
        if !file.namespace.is_empty() {
            candidates.push(global_variable_key(&reference.text, &file.namespace));
        }
        candidates.push(lower);
        candidates
            .into_iter()
            .find(|key| self.variables.contains(key))
            .map(ResolvedVariable::Global)
    }

    fn add_section(&mut self, kind: SymbolKind, qualified_name: String) {
        self.sections
            .entry(qualified_name.to_lowercase())
            .or_default()
            .push(SectionSymbol {
                kind,
                qualified_name,
            });
    }
}

/// The namespace named explicitly in a reference (`CommandList\ZZMI\Foo` is `ZZMI`,
/// `$\SlotFix\Matches\slot` is `SlotFix\Matches`), or `None` when it has none.
pub fn explicit_namespace(text: &str, lookup: &Lookup) -> Option<String> {
    let rest = match text.strip_prefix('$') {
        Some(rest) => rest,
        None => text.get(lookup.section_prefix(text)?.len()..)?,
    };
    if !rest.starts_with('\\') {
        return None;
    }

    let last = rest.rfind('\\')?;
    (last > 0).then(|| rest[1..last].to_string())
}
