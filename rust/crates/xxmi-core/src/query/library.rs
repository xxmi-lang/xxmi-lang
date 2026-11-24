//! Library lookups for `xxmi query symbol|library`: what a package's public symbols are, where
//! they live, and what documents them.

use std::path::Path;

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use crate::lookup::Lookup;
use crate::model::{IniFile, SymbolKind};
use crate::paths::relative_path;
use crate::workspace::Workspace;
use crate::workspace::snapshot::SymbolSnapshot;
use crate::workspace::symbols::{
    Origin, SectionSymbol, SymbolTable, VariableSymbol, explicit_namespace,
};

use super::docs::{DocComment, comment_blocks, doc_above, split_lines};
use super::utf16_cmp;

const SOURCE_LINE_CAP: usize = 80;
const USAGE_CAP: usize = 5;
const USAGE_LINE_CAP: usize = 30;
const MEMBER_CAP: usize = 80;

#[derive(Debug, Serialize)]
pub struct SymbolLocation {
    /// Relative to the package root, `/`-separated.
    pub path: String,
    /// 1-based.
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct SymbolUsage {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SymbolMember {
    pub kind: SymbolKind,
    pub name: String,
    pub path: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct SymbolReference {
    pub kind: SymbolKind,
    pub text: String,
    pub access: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo {
    pub kind: SymbolKind,
    /// As the DLL stores it, e.g. `CommandList\ZZMI\SetTextures`.
    pub qualified_name: String,
    pub location: SymbolLocation,
    /// Only the location is known: the symbol comes from a bundled snapshot.
    pub snapshot_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocComment>,
    /// The comment block at the top of the declaring file (usually a `Usage:` block).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_doc: Option<SymbolUsage>,
    /// The section body (or the declaration line), capped at `SOURCE_LINE_CAP` lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub references: Vec<SymbolReference>,
    /// Comment blocks elsewhere in the package that mention the symbol by its full name.
    pub usage: Vec<SymbolUsage>,
    /// Other symbols in the same namespace: the inputs and outputs libraries share.
    pub namespace_members: Vec<SymbolMember>,
}

impl SymbolInfo {
    fn bare(kind: SymbolKind, qualified_name: String, location: SymbolLocation) -> Self {
        Self {
            kind,
            qualified_name,
            location,
            snapshot_only: false,
            doc: None,
            module_doc: None,
            source: None,
            references: Vec::new(),
            usage: Vec::new(),
            namespace_members: Vec::new(),
        }
    }
}

/// A package loaded into a workspace, with the root its paths are shown relative to.
pub struct LoadedPackage<'a> {
    pub workspace: &'a Workspace,
    pub root: &'a Path,
}

impl LoadedPackage<'_> {
    fn symbols(&self) -> &SymbolTable {
        self.workspace.symbols_at(Some(self.root))
    }

    fn rel(&self, path: &Path) -> String {
        relative_path(self.root, path).replace(std::path::MAIN_SEPARATOR, "/")
    }
}

enum Found<'a> {
    Section(&'a SectionSymbol),
    Variable(&'a VariableSymbol),
}

fn lookup_symbol<'a>(symbols: &'a SymbolTable, name: &str) -> Option<Found<'a>> {
    let lower = name.to_lowercase();
    symbols
        .all_sections()
        .find(|s| s.qualified_name.to_lowercase() == lower)
        .map(Found::Section)
        .or_else(|| {
            symbols
                .all_variables()
                .find(|v| v.key == lower)
                .map(Found::Variable)
        })
}

fn references_of(file: &IniFile, section_index: usize) -> Vec<SymbolReference> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for r in file
        .references
        .iter()
        .filter(|r| r.section == Some(section_index))
    {
        let access = match r.access {
            crate::model::Access::Read => "read",
            crate::model::Access::Write => "write",
        };
        if seen.insert((r.kind, r.text.to_lowercase(), access)) {
            out.push(SymbolReference {
                kind: r.kind,
                text: r.text.clone(),
                access,
            });
        }
    }
    out
}

fn module_doc_of(pkg: &LoadedPackage, file: &IniFile) -> Option<SymbolUsage> {
    let first_section = file
        .sections
        .first()
        .map_or(usize::MAX, |s| s.header.line as usize);
    let lines = split_lines(&file.text);
    let block = comment_blocks(&lines)
        .into_iter()
        .find(|b| b.line - 1 < first_section)?;
    Some(SymbolUsage {
        path: pkg.rel(&file.path),
        line: block.line,
        text: block.text,
    })
}

fn usage_of(pkg: &LoadedPackage, qualified_name: &str, own: &SymbolLocation) -> Vec<SymbolUsage> {
    let needle = qualified_name.to_lowercase();
    let mut out = Vec::new();

    for file in pkg.workspace.files() {
        let path = pkg.rel(&file.path);
        for block in comment_blocks(&split_lines(&file.text)) {
            if !block.text.to_lowercase().contains(&needle) {
                continue;
            }
            // The block directly above the declaration is its own doc, not a usage example.
            if path == own.path && block.end_line + 1 == own.line {
                continue;
            }

            let text: Vec<&str> = block.text.split('\n').take(USAGE_LINE_CAP).collect();
            out.push(SymbolUsage {
                path: path.clone(),
                line: block.line,
                text: text.join("\n"),
            });
            if out.len() >= USAGE_CAP {
                return out;
            }
        }
    }
    out
}

fn members_of(pkg: &LoadedPackage, lookup: &Lookup, qualified_name: &str) -> Vec<SymbolMember> {
    let Some(namespace) = explicit_namespace(qualified_name, lookup).map(|n| n.to_lowercase())
    else {
        return Vec::new();
    };
    let same_namespace = |name: &str| {
        explicit_namespace(name, lookup).is_some_and(|n| n.to_lowercase() == namespace)
    };

    let own = qualified_name.to_lowercase();
    let mut members = Vec::new();
    for file in pkg.workspace.files() {
        for section in &file.sections {
            let Some(kind) = section.kind else { continue };
            if section.qualified_name.to_lowercase() == own
                || !same_namespace(&section.qualified_name)
            {
                continue;
            }
            members.push(SymbolMember {
                kind,
                name: section.qualified_name.clone(),
                path: pkg.rel(&file.path),
                line: section.header.line as usize + 1,
            });
        }
    }

    for variable in pkg.symbols().all_variables() {
        let (Origin::File { file, .. }, true) = (&variable.origin, same_namespace(&variable.key))
        else {
            continue;
        };
        let file = &pkg.workspace.files()[*file];
        let declaration = variable.declaration.map(|(_, i)| &file.variables[i]);
        members.push(SymbolMember {
            kind: SymbolKind::Variable,
            name: declaration.map_or_else(|| variable.key.clone(), |d| d.name.clone()),
            path: pkg.rel(&file.path),
            line: declaration.map_or(0, |d| file.range_of(d.span).start.line as usize) + 1,
        });
    }

    members.truncate(MEMBER_CAP);
    members
}

fn section_info(pkg: &LoadedPackage, symbol: &SectionSymbol) -> Option<SymbolInfo> {
    let Origin::File { file: id, section } = symbol.origin else {
        return None;
    };
    let index = section?;
    let file = &pkg.workspace.files()[id];
    let section = file.sections.get(index)?;

    let lines = split_lines(&file.text);
    let header = section.header.line as usize;
    let end = file
        .sections
        .get(index + 1)
        .map_or(lines.len(), |next| next.header.line as usize);
    let body = &lines[header.min(lines.len())..end.min(header + SOURCE_LINE_CAP).min(lines.len())];

    let location = SymbolLocation {
        path: pkg.rel(&file.path),
        line: header + 1,
    };
    let mut info = SymbolInfo::bare(symbol.kind, symbol.qualified_name.clone(), location);
    info.doc = doc_above(&lines, header);
    info.module_doc = module_doc_of(pkg, file);
    info.source = Some(body.join("\n").trim_end().to_string());
    info.references = references_of(file, index);
    Some(info)
}

fn variable_info(pkg: &LoadedPackage, symbol: &VariableSymbol) -> Option<SymbolInfo> {
    let (Origin::File { file: id, .. }, Some((_, declared))) = (&symbol.origin, symbol.declaration)
    else {
        return None;
    };
    let file = &pkg.workspace.files()[*id];
    let declaration = &file.variables[declared];
    let line = file.range_of(declaration.span).start.line as usize;
    let lines = split_lines(&file.text);

    let location = SymbolLocation {
        path: pkg.rel(&file.path),
        line: line + 1,
    };
    let mut info = SymbolInfo::bare(SymbolKind::Variable, declaration.name.clone(), location);
    info.doc = doc_above(&lines, line);
    info.module_doc = module_doc_of(pkg, file);
    info.source = Some(lines.get(line).map_or("", |l| l.trim()).to_string());
    Some(info)
}

/// Describes the symbol with this qualified name (`CommandList\ZZMI\SetTextures`,
/// `$\TTL\alpha`) in a loaded package, or `None` when the package doesn't declare it.
pub fn describe_symbol(pkg: &LoadedPackage, name: &str) -> Option<SymbolInfo> {
    let found = lookup_symbol(pkg.symbols(), name.trim())?;
    let (mut info, key) = match found {
        Found::Section(section) => {
            let info = section_info(pkg, section)?;
            let key = info.qualified_name.clone();
            (info, key)
        }
        Found::Variable(variable) => (variable_info(pkg, variable)?, variable.key.clone()),
    };

    info.usage = usage_of(pkg, &key, &info.location);
    info.namespace_members = members_of(pkg, pkg.workspace.lookup(), &key);
    Some(info)
}

/// The location a snapshot records for `name`; no docs (the snapshot doesn't carry them).
pub fn describe_snapshot_symbol(snapshot: &SymbolSnapshot, name: &str) -> Option<SymbolInfo> {
    let lower = name.trim().to_lowercase();
    let (kind, qualified_name, path, line) = match snapshot
        .sections
        .iter()
        .find(|s| s.qualified_name.to_lowercase() == lower)
    {
        Some(s) => (s.kind, s.qualified_name.clone(), s.path.clone(), s.line),
        None => {
            let v = snapshot.variables.iter().find(|v| v.key == lower)?;
            (SymbolKind::Variable, v.key.clone(), v.path.clone(), v.line)
        }
    };

    let location = SymbolLocation {
        path,
        line: line as usize,
    };
    let mut info = SymbolInfo::bare(kind, qualified_name, location);
    info.snapshot_only = true;
    Some(info)
}

/// Symbol counts by kind, listed in the order the kinds first appeared (a JavaScript object).
#[derive(Debug, Default)]
pub struct KindCounts(pub Vec<(SymbolKind, usize)>);

impl KindCounts {
    fn add(&mut self, kind: SymbolKind) {
        match self.0.iter_mut().find(|(k, _)| *k == kind) {
            Some(slot) => slot.1 += 1,
            None => self.0.push((kind, 1)),
        }
    }
}

impl Serialize for KindCounts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (kind, count) in &self.0 {
            map.serialize_entry(kind.as_str(), count)?;
        }
        map.end()
    }
}

#[derive(Debug, Serialize)]
pub struct LibraryInfo {
    pub namespace: String,
    pub counts: KindCounts,
    /// Files of the namespace and the doc block at the top of the first one that has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(rename = "moduleDoc", skip_serializing_if = "Option::is_none")]
    pub module_doc: Option<SymbolUsage>,
}

/// Namespaces that declare symbols, with symbol counts; docs and files when the package is loaded.
pub fn list_libraries(
    snapshot: &SymbolSnapshot,
    lookup: &Lookup,
    pkg: Option<&LoadedPackage>,
) -> Vec<LibraryInfo> {
    let entries = snapshot
        .sections
        .iter()
        .map(|s| (s.kind, s.qualified_name.as_str()))
        .chain(
            snapshot
                .variables
                .iter()
                .map(|v| (SymbolKind::Variable, v.key.as_str())),
        );

    let mut libraries: Vec<LibraryInfo> = Vec::new();
    for (kind, name) in entries {
        let Some(namespace) = explicit_namespace(name, lookup).map(|n| n.to_lowercase()) else {
            continue;
        };
        let Some(original) = snapshot
            .namespaces
            .iter()
            .find(|n| n.to_lowercase() == namespace)
        else {
            continue;
        };

        let position = libraries.iter().position(|l| l.namespace == *original);
        let index = position.unwrap_or_else(|| {
            libraries.push(LibraryInfo {
                namespace: original.clone(),
                counts: KindCounts::default(),
                files: None,
                module_doc: None,
            });
            libraries.len() - 1
        });
        libraries[index].counts.add(kind);
    }

    libraries.sort_by(|a, b| locale_cmp(&a.namespace, &b.namespace));
    if let Some(pkg) = pkg {
        for library in &mut libraries {
            add_files(library, pkg);
        }
    }
    libraries
}

fn add_files(library: &mut LibraryInfo, pkg: &LoadedPackage) {
    let namespace = library.namespace.to_lowercase();
    let files: Vec<&IniFile> = pkg
        .workspace
        .files()
        .iter()
        .filter(|f| f.namespace.to_lowercase() == namespace)
        .collect();

    let mut paths: Vec<String> = files.iter().map(|f| pkg.rel(&f.path)).collect();
    paths.sort_by(|a, b| utf16_cmp(a, b));
    library.files = Some(paths);
    library.module_doc = files.iter().find_map(|f| module_doc_of(pkg, f));
}

/// Approximates `localeCompare` for namespace names: punctuation, then digits, then letters
/// (case-insensitively), with lower case before upper case only as a tie-break.
fn locale_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let primary = |text: &str| -> Vec<(u8, char)> {
        text.chars()
            .map(|c| {
                let class = match c {
                    c if c.is_alphabetic() => 2,
                    c if c.is_numeric() => 1,
                    _ => 0,
                };
                (class, c.to_lowercase().next().unwrap_or(c))
            })
            .collect()
    };

    primary(a).cmp(&primary(b)).then_with(|| {
        let first_case_difference = a.chars().zip(b.chars()).find(|(x, y)| x != y);
        match first_case_difference {
            Some((x, _)) if x.is_lowercase() => std::cmp::Ordering::Less,
            Some(_) => std::cmp::Ordering::Greater,
            None => std::cmp::Ordering::Equal,
        }
    })
}
