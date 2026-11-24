//! Rename of sections and variables.

use std::path::PathBuf;

use crate::diagnostic::{Position, Range, TextEdit};
use crate::model::IniFile;
use crate::scan::Span;
use crate::workspace::symbols::Origin;
use crate::workspace::{FileId, Workspace};

use super::target::{Symbol, Target, contains, matching_references, target_at};

pub struct RenameOutcome {
    /// The name part under the cursor.
    pub range: Range,
    pub placeholder: String,
    /// Edits by file, in the order the files were first touched; empty without a new name.
    pub edits: Vec<(PathBuf, Vec<TextEdit>)>,
}

/// Where the renameable name part of a reference or header starts, in bytes of `text`.
fn name_part_start(text: &str, workspace: &Workspace, is_variable: bool) -> usize {
    if let Some(slash) = text.rfind('\\') {
        return slash + 1;
    }
    if is_variable {
        return 1;
    }
    workspace.lookup().section_prefix(text).map_or(0, str::len)
}

/// d3dx.ini and `Core/`: libraries other mods call.
fn is_package_file(workspace: &Workspace, file: &IniFile) -> bool {
    if file.namespace.is_empty() {
        return true;
    }
    let Some(root) = workspace.package_root(file) else {
        return false;
    };
    file.path
        .strip_prefix(root)
        .ok()
        .and_then(|rest| rest.components().next())
        .is_some_and(|first| first.as_os_str().to_string_lossy().to_lowercase() == "core")
}

fn is_section_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_whitespace() || "\\[]=,;$".contains(c))
}

fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn add_edit(edits: &mut Vec<(PathBuf, Vec<TextEdit>)>, file: &IniFile, span: Span, new_text: &str) {
    let edit = TextEdit {
        range: file.range_of(span),
        new_text: new_text.to_string(),
    };
    match edits.iter_mut().find(|(path, _)| *path == file.path) {
        Some((_, list)) => list.push(edit),
        None => edits.push((file.path.clone(), vec![edit])),
    }
}

/// Everything about the declaration that rename needs.
struct Declaration {
    file: FileId,
    /// The name part.
    name: Span,
    /// The whole declared name (header text or `$var`).
    whole: Span,
}

fn declaration_of(workspace: &Workspace, symbol: &Symbol) -> Result<Declaration, String> {
    let missing = || "Declaration not found.".to_string();
    let Origin::File { file: id, section } = symbol.origin() else {
        return Err(missing());
    };
    let file = &workspace.files()[*id];

    let (whole, text, is_variable) = match symbol {
        Symbol::Section(_) => {
            let section = file
                .sections
                .get((*section).ok_or_else(missing)?)
                .ok_or_else(missing)?;
            (section.name, file.slice(section.name), false)
        }
        Symbol::Variable(variable) => {
            let (declaration_file, index) = variable.declaration.ok_or_else(missing)?;
            let declaration = &workspace.files()[declaration_file].variables[index];
            (declaration.span, declaration.name.as_str(), true)
        }
    };
    let start = whole.start + name_part_start(text, workspace, is_variable);
    Ok(Declaration {
        file: *id,
        name: Span {
            start,
            end: whole.end,
        },
        whole,
    })
}

fn not_renameable(workspace: &Workspace, symbol: &Symbol) -> Option<String> {
    let name = match symbol {
        Symbol::Section(section) => &section.qualified_name,
        Symbol::Variable(variable) => &variable.key,
    };
    match symbol.origin() {
        Origin::Snapshot { name: snapshot, .. } => {
            let source = if snapshot == "builtin" {
                "DLL".to_string()
            } else {
                format!("{snapshot} library snapshot")
            };
            Some(format!(
                "`{name}` comes from the {source} and can't be renamed."
            ))
        }
        Origin::File { file: id, .. } => {
            let file = &workspace.files()[*id];
            (is_package_file(workspace, file) && !workspace.is_target(file)).then(|| {
                format!(
                    "This symbol is declared in the package ({}); rename it there.",
                    file.path.display()
                )
            })
        }
    }
}

fn invalid_name_message(is_variable: bool) -> &'static str {
    if is_variable {
        "Variable names are letters, digits and `_`, not starting with a digit."
    } else {
        "Section names can't contain spaces, `\\`, `[`, `]`, `=`, `,`, `;` or `$`."
    }
}

fn conflicts(
    workspace: &Workspace,
    file: &IniFile,
    symbol: &Symbol,
    placeholder: &str,
    new_name: &str,
) -> bool {
    let stem = |name: &str| {
        name.get(..name.len().saturating_sub(placeholder.len()))
            .unwrap_or("")
            .to_string()
    };
    let found = match symbol {
        Symbol::Section(section) => {
            let renamed = stem(&section.qualified_name) + new_name;
            !workspace
                .symbols_for(file)
                .sections_named(&renamed)
                .is_empty()
        }
        Symbol::Variable(variable) => {
            let renamed = stem(&variable.key) + &new_name.to_lowercase();
            workspace
                .symbols_for(file)
                .all_variables()
                .any(|v| v.key == renamed)
        }
    };
    found && new_name.to_lowercase() != placeholder.to_lowercase()
}

/// Renames a section (and every `run =`, `ref`, `Prefix\N\Name` reference) or a variable (`$x`,
/// `$\N\x`). Only the name part changes; prefixes and namespaces are kept as written. Symbols
/// declared in the package or a snapshot can't be renamed from a mod. Without `new_name` it only
/// checks that the symbol under `pos` can be renamed.
pub fn rename(
    workspace: &Workspace,
    id: FileId,
    pos: Position,
    new_name: Option<&str>,
) -> Result<RenameOutcome, String> {
    let file = &workspace.files()[id];
    let Some(Target::Symbol { symbol, range }) = target_at(workspace, id, pos) else {
        return Err("Nothing to rename here.".to_string());
    };
    if let Some(message) = not_renameable(workspace, &symbol) {
        return Err(message);
    }

    let offset = file.offset_at(pos);
    let declaration = declaration_of(workspace, &symbol)?;
    let declaring = &workspace.files()[declaration.file];
    let placeholder = declaring.slice(declaration.name).to_string();

    let mut edits = Vec::new();
    let mut placeholder_range = range;
    if let Some(new_name) = new_name {
        add_edit(&mut edits, declaring, declaration.name, new_name);
    }
    if declaration.file == id && offset.is_some_and(|o| contains(declaration.whole, o)) {
        placeholder_range = file.range_of(declaration.name);
    }

    for (other_id, reference) in matching_references(workspace, &symbol) {
        let other = &workspace.files()[other_id];
        let start = reference.span.start
            + name_part_start(&reference.text, workspace, symbol.is_variable());
        let name = Span {
            start,
            end: reference.span.start + reference.text.len(),
        };
        if let Some(new_name) = new_name {
            add_edit(&mut edits, other, name, new_name);
        }
        if other_id == id && offset.is_some_and(|o| contains(reference.span, o)) {
            placeholder_range = other.range_of(name);
        }
    }

    if let Some(new_name) = new_name {
        validate(workspace, file, &symbol, &placeholder, new_name)?;
    }
    Ok(RenameOutcome {
        range: placeholder_range,
        placeholder,
        edits,
    })
}

fn validate(
    workspace: &Workspace,
    file: &IniFile,
    symbol: &Symbol,
    placeholder: &str,
    new_name: &str,
) -> Result<(), String> {
    let is_variable = symbol.is_variable();
    let valid = if is_variable {
        is_variable_name(new_name)
    } else {
        is_section_name(new_name)
    };
    if !valid {
        return Err(invalid_name_message(is_variable).to_string());
    }
    if conflicts(workspace, file, symbol, placeholder, new_name) {
        return Err(format!(
            "`{new_name}` is already declared in that namespace."
        ));
    }
    Ok(())
}
