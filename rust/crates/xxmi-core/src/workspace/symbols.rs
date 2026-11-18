//! Workspace-wide symbols and DLL-order name resolution (port of packages/core/src/workspace/symbols.ts,
//! docs/01-language-model.md §3 rules 1-7).
//!
//! The DLL stores a prefixed section from a namespaced file as `Prefix\namespace\Local` and a
//! global declared in namespace N as `$\n\name`. A reference first tries the name qualified with the
//! referencing file's namespace, then the name as written. All comparisons are case-insensitive.

use std::collections::{HashMap, HashSet};

use crate::lookup::Lookup;
use crate::model::{IniFile, Reference, Scope, SymbolKind};

use super::FileId;
use super::snapshot::SymbolSnapshot;

/// Where a symbol is declared: a loaded file, or an entry of a library snapshot (the DLL's
/// built-in sections use the snapshot name `builtin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    File {
        file: FileId,
        section: Option<usize>,
    },
    Snapshot {
        name: String,
        path: String,
        line: u32,
    },
}

#[derive(Debug, Clone)]
pub struct SectionSymbol {
    pub kind: SymbolKind,
    /// As the DLL stores it, e.g. `CommandList\ZZMI\SetTextures`.
    pub qualified_name: String,
    pub origin: Origin,
}

#[derive(Debug, Clone)]
pub struct VariableSymbol {
    /// As the DLL stores it, lower-cased: `$\ttl\alpha`, or `$name` for d3dx.ini globals.
    pub key: String,
    pub persist: bool,
    pub locked: bool,
    pub origin: Origin,
    /// File and index into `IniFile::variables`.
    pub declaration: Option<(FileId, usize)>,
}

/// What a variable reference resolved to; all the rules need is which declaration was used.
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

/// Lists grouped by key, iterated in the order the keys first appeared (a JavaScript `Map`).
struct Grouped<V> {
    order: Vec<String>,
    groups: HashMap<String, Vec<V>>,
}

impl<V> Grouped<V> {
    fn new() -> Self {
        Self {
            order: Vec::new(),
            groups: HashMap::new(),
        }
    }

    fn push(&mut self, key: String, value: V) {
        match self.groups.get_mut(&key) {
            Some(list) => list.push(value),
            None => {
                self.order.push(key.clone());
                self.groups.insert(key, vec![value]);
            }
        }
    }

    fn get(&self, key: &str) -> &[V] {
        self.groups.get(key).map_or(&[], Vec::as_slice)
    }

    fn values(&self) -> impl Iterator<Item = &V> {
        self.order.iter().flat_map(|key| &self.groups[key])
    }
}

pub struct SymbolTable {
    lookup: &'static Lookup,
    sections: Grouped<SectionSymbol>,
    variables: Grouped<VariableSymbol>,
    namespaces: HashSet<String>,
}

impl SymbolTable {
    pub fn new(lookup: &'static Lookup) -> Self {
        Self {
            lookup,
            sections: Grouped::new(),
            variables: Grouped::new(),
            namespaces: HashSet::new(),
        }
    }

    pub fn add_file(&mut self, id: FileId, file: &IniFile) {
        if !file.namespace.is_empty() {
            self.namespaces.insert(file.namespace.to_lowercase());
        }

        for (index, section) in file.sections.iter().enumerate() {
            if let Some(kind) = section.kind {
                self.add_section(SectionSymbol {
                    kind,
                    qualified_name: section.qualified_name.clone(),
                    origin: Origin::File {
                        file: id,
                        section: Some(index),
                    },
                });
            }
        }
        for (index, declaration) in file.variables.iter().enumerate() {
            if declaration.scope != Scope::Global {
                continue;
            }
            self.add_variable(VariableSymbol {
                key: global_variable_key(&declaration.name, &file.namespace),
                persist: declaration.persist,
                locked: declaration.locked,
                origin: Origin::File {
                    file: id,
                    section: declaration.section,
                },
                declaration: Some((id, index)),
            });
        }
    }

    pub fn add_snapshot(&mut self, snapshot: &SymbolSnapshot) {
        for namespace in &snapshot.namespaces {
            self.namespaces.insert(namespace.to_lowercase());
        }
        for section in &snapshot.sections {
            self.add_section(SectionSymbol {
                kind: section.kind,
                qualified_name: section.qualified_name.clone(),
                origin: snapshot_origin(snapshot, &section.path, section.line),
            });
        }
        for variable in &snapshot.variables {
            self.add_variable(VariableSymbol {
                key: variable.key.clone(),
                persist: variable.persist,
                locked: variable.locked,
                origin: snapshot_origin(snapshot, &variable.path, variable.line),
                declaration: None,
            });
        }
    }

    /// Whether a loaded file or snapshot declares this namespace.
    pub fn has_namespace(&self, namespace: &str) -> bool {
        self.namespaces.contains(&namespace.to_lowercase())
    }

    /// All sections declared under a qualified name (more than one means a duplicate).
    pub fn sections_named(&self, qualified_name: &str) -> &[SectionSymbol] {
        self.sections.get(&qualified_name.to_lowercase())
    }

    pub fn all_sections(&self) -> impl Iterator<Item = &SectionSymbol> {
        self.sections.values()
    }

    pub fn all_variables(&self) -> impl Iterator<Item = &VariableSymbol> {
        self.variables.values()
    }

    /// Resolves a section reference made from `namespace`. The DLL's own built-in sections resolve
    /// to a symbol too.
    pub fn resolve_section(&self, kind: SymbolKind, text: &str, namespace: &str) -> Option<String> {
        self.resolve_section_symbol(kind, text, namespace)
            .map(|symbol| symbol.qualified_name)
    }

    pub fn resolve_section_symbol(
        &self,
        kind: SymbolKind,
        text: &str,
        namespace: &str,
    ) -> Option<SectionSymbol> {
        for candidate in self.section_candidates(text, namespace) {
            let found = self
                .sections
                .get(&candidate.to_lowercase())
                .iter()
                .find(|s| s.kind == kind);
            if let Some(found) = found {
                return Some(found.clone());
            }
        }
        self.lookup.is_builtin_section(text).then(|| SectionSymbol {
            kind,
            qualified_name: text.to_string(),
            origin: Origin::Snapshot {
                name: "builtin".to_string(),
                path: "d3d11.dll".to_string(),
                line: 0,
            },
        })
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
        if let Some(local) = find_local(reference, file) {
            return Some(ResolvedVariable::Local(file.variables[local].span.start));
        }
        self.find_global(reference, file)
            .map(|symbol| ResolvedVariable::Global(symbol.key.clone()))
    }

    /// `resolve_variable` with the symbol, for navigation. `id` is the file the reference is in.
    pub fn resolve_variable_symbol(
        &self,
        reference: &Reference,
        id: FileId,
        file: &IniFile,
    ) -> Option<VariableSymbol> {
        if let Some(local) = find_local(reference, file) {
            return Some(VariableSymbol {
                key: reference.text.to_lowercase(),
                persist: false,
                locked: false,
                origin: Origin::File {
                    file: id,
                    section: file.variables[local].section,
                },
                declaration: Some((id, local)),
            });
        }
        self.find_global(reference, file).cloned()
    }

    fn find_global(&self, reference: &Reference, file: &IniFile) -> Option<&VariableSymbol> {
        let mut candidates = Vec::new();
        if !file.namespace.is_empty() {
            candidates.push(global_variable_key(&reference.text, &file.namespace));
        }
        candidates.push(reference.text.to_lowercase());
        candidates
            .iter()
            .find_map(|key| self.variables.get(key).first())
    }

    fn add_section(&mut self, symbol: SectionSymbol) {
        self.sections
            .push(symbol.qualified_name.to_lowercase(), symbol);
    }

    fn add_variable(&mut self, symbol: VariableSymbol) {
        self.variables.push(symbol.key.clone(), symbol);
    }
}

/// The local declared earlier in the reference's own section under the same name.
fn find_local(reference: &Reference, file: &IniFile) -> Option<usize> {
    let lower = reference.text.to_lowercase();
    file.variables.iter().position(|v| {
        v.scope == Scope::Local
            && v.section == reference.section
            && v.span.start <= reference.span.start
            && v.name.to_lowercase() == lower
    })
}

fn snapshot_origin(snapshot: &SymbolSnapshot, path: &str, line: u32) -> Origin {
    Origin::Snapshot {
        name: snapshot.name.clone(),
        path: path.to_string(),
        line,
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
