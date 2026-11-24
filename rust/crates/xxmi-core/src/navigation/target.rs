//! What the cursor is on, and its definition and references.

use std::path::PathBuf;

use crate::diagnostic::{Position, Range};
use crate::model::{IniFile, Reference, Scope, SymbolKind, VariableDeclaration};
use crate::scan::Span;
use crate::workspace::symbols::{Origin, SectionSymbol, VariableSymbol, global_variable_key};
use crate::workspace::{FileId, Workspace};

#[derive(Debug, Clone)]
pub struct Location {
    pub path: PathBuf,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum Symbol {
    Section(SectionSymbol),
    Variable(VariableSymbol),
}

impl Symbol {
    pub fn origin(&self) -> &Origin {
        match self {
            Self::Section(symbol) => &symbol.origin,
            Self::Variable(symbol) => &symbol.origin,
        }
    }

    pub fn is_variable(&self) -> bool {
        matches!(self, Self::Variable(_))
    }
}

#[derive(Debug, Clone)]
pub enum Target {
    Symbol {
        symbol: Symbol,
        range: Range,
    },
    /// An `include` or `filename` value that resolves to a file.
    Path {
        path: PathBuf,
        range: Range,
    },
}

/// Positions compare as (line, column), and a range includes both of its ends.
pub fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

/// The symbol a reference resolves to from `id`.
pub fn resolve_ref(workspace: &Workspace, id: FileId, reference: &Reference) -> Option<Symbol> {
    let file = &workspace.files()[id];
    let symbols = workspace.symbols_for(file);

    if reference.kind == SymbolKind::Variable {
        return symbols
            .resolve_variable_symbol(reference, id, file)
            .map(Symbol::Variable);
    }
    symbols
        .resolve_section_symbol(reference.kind, &reference.text, &file.namespace)
        .map(Symbol::Section)
}

/// What the cursor is on: a reference, a declaration, or an include/filename path.
pub fn target_at(workspace: &Workspace, id: FileId, pos: Position) -> Option<Target> {
    let file = &workspace.files()[id];
    let offset = file.offset_at(pos)?;

    if let Some(reference) = file.references.iter().find(|r| contains(r.span, offset)) {
        let symbol = resolve_ref(workspace, id, reference)?;
        return Some(Target::Symbol {
            symbol,
            range: file.range_of(reference.span),
        });
    }
    declaration_at(workspace, id, offset).or_else(|| path_at(workspace, id, offset))
}

fn declaration_at(workspace: &Workspace, id: FileId, offset: usize) -> Option<Target> {
    let file = &workspace.files()[id];

    let section = file
        .sections
        .iter()
        .position(|s| contains(s.name, offset))
        .and_then(|index| Some((index, own_symbol(workspace, id, index)?)));
    if let Some((index, symbol)) = section {
        return Some(Target::Symbol {
            symbol: Symbol::Section(symbol),
            range: file.range_of(file.sections[index].name),
        });
    }

    let index = file
        .variables
        .iter()
        .position(|v| contains(v.span, offset))?;
    let symbol = variable_symbol_of(workspace, id, index)?;
    Some(Target::Symbol {
        symbol: Symbol::Variable(symbol),
        range: file.range_of(file.variables[index].span),
    })
}

/// The section symbol declared by a section of a loaded file.
fn own_symbol(workspace: &Workspace, id: FileId, index: usize) -> Option<SectionSymbol> {
    let file = &workspace.files()[id];
    let section = file.sections.get(index)?;
    section.kind?;

    workspace
        .symbols_for(file)
        .sections_named(&section.qualified_name)
        .iter()
        .find(|s| {
            s.origin
                == Origin::File {
                    file: id,
                    section: Some(index),
                }
        })
        .cloned()
}

fn variable_symbol_of(workspace: &Workspace, id: FileId, index: usize) -> Option<VariableSymbol> {
    let file = &workspace.files()[id];
    let declaration: &VariableDeclaration = &file.variables[index];

    if declaration.scope == Scope::Local {
        return Some(VariableSymbol {
            key: declaration.name.to_lowercase(),
            persist: false,
            locked: false,
            origin: Origin::File {
                file: id,
                section: declaration.section,
            },
            declaration: Some((id, index)),
        });
    }

    let key = global_variable_key(&declaration.name, &file.namespace);
    workspace
        .symbols_for(file)
        .all_variables()
        .find(|v| v.key == key && v.declaration == Some((id, index)))
        .cloned()
}

fn path_at(workspace: &Workspace, id: FileId, offset: usize) -> Option<Target> {
    let file = &workspace.files()[id];

    for section in &file.sections {
        let kind = section.spec.map(|s| s.name.as_str());
        for line in &section.lines {
            let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            if value.start == value.end || !contains(value, offset) {
                continue;
            }

            let key = file.slice(key).to_lowercase();
            let is_include =
                kind == Some("Include") && (key == "include" || key == "include_recursive");
            let is_filename = matches!(kind, Some("Resource" | "Pool")) && key == "filename";
            if !is_include && !is_filename {
                continue;
            }

            if let Some(path) = resolve_value_path(workspace, file, file.slice(value), is_filename)
            {
                return Some(Target::Path {
                    path,
                    range: file.range_of(value),
                });
            }
        }
    }
    None
}

/// Relative to the including file, then (for `filename`) to the package root.
fn resolve_value_path(
    workspace: &Workspace,
    file: &IniFile,
    value: &str,
    is_filename: bool,
) -> Option<PathBuf> {
    let resolver = workspace.resolver();
    let dir = file.path.parent()?;
    resolver.resolve(dir, value).or_else(|| {
        let root = workspace.package_root(file)?;
        is_filename.then(|| resolver.resolve(root, value)).flatten()
    })
}

/// Where a target is declared. Snapshot and built-in symbols have no location.
pub fn definition(workspace: &Workspace, target: &Target) -> Option<Location> {
    let symbol = match target {
        Target::Path { path, .. } => {
            let start = Position {
                line: 0,
                character: 0,
            };
            return Some(Location {
                path: path.clone(),
                range: Range { start, end: start },
            });
        }
        Target::Symbol { symbol, .. } => symbol,
    };
    let Origin::File { file: id, section } = symbol.origin() else {
        return None;
    };
    let file = &workspace.files()[*id];

    let span = match symbol {
        Symbol::Variable(variable) => {
            let (declaration_file, index) = variable.declaration?;
            workspace.files()[declaration_file].variables[index].span
        }
        Symbol::Section(_) => file.sections.get((*section)?)?.name,
    };
    Some(Location {
        path: file.path.clone(),
        range: file.range_of(span),
    })
}

fn is_local(workspace: &Workspace, symbol: &VariableSymbol) -> bool {
    symbol
        .declaration
        .is_some_and(|(file, index)| workspace.files()[file].variables[index].scope == Scope::Local)
}

pub fn same_symbol(workspace: &Workspace, a: &Symbol, b: &Symbol) -> bool {
    match (a, b) {
        (Symbol::Section(a), Symbol::Section(b)) => {
            a.kind == b.kind && a.qualified_name.to_lowercase() == b.qualified_name.to_lowercase()
        }
        (Symbol::Variable(a), Symbol::Variable(b)) => {
            if is_local(workspace, a) || is_local(workspace, b) {
                a.declaration == b.declaration
            } else {
                a.key == b.key
            }
        }
        _ => false,
    }
}

/// Every reference to `symbol` in the workspace, in load order.
pub fn matching_references<'a>(
    workspace: &'a Workspace,
    symbol: &'a Symbol,
) -> impl Iterator<Item = (FileId, &'a Reference)> {
    workspace
        .files()
        .iter()
        .enumerate()
        .flat_map(move |(id, file)| file.references.iter().map(move |r| (id, r)))
        .filter(move |(id, reference)| {
            (reference.kind == SymbolKind::Variable) == symbol.is_variable()
                && resolve_ref(workspace, *id, reference)
                    .is_some_and(|resolved| same_symbol(workspace, &resolved, symbol))
        })
}

/// Every reference to `target` in the workspace, plus its declaration if asked.
pub fn references(
    workspace: &Workspace,
    target: &Target,
    include_declaration: bool,
) -> Vec<Location> {
    let Target::Symbol { symbol, .. } = target else {
        return Vec::new();
    };

    let mut found = Vec::new();
    if include_declaration {
        found.extend(definition(workspace, target));
    }
    for (id, reference) in matching_references(workspace, symbol) {
        let file = &workspace.files()[id];
        found.push(Location {
            path: file.path.clone(),
            range: file.range_of(reference.span),
        });
    }
    found
}
