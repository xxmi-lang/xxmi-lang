//! The declaration of each global the DLL keeps, shared by XM211.

use std::collections::HashMap;

use super::RuleContext;
use super::duplicate_section::relative_location;
use super::global_redeclared::constants_globals;
use super::loaded_files::{loaded_files, loads_before};
use crate::workspace::FileId;
use crate::workspace::symbols::global_variable_key;

/// A declaration, as file and index into `IniFile::variables`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Site {
    pub file: FileId,
    pub index: usize,
}

pub struct Kept {
    pub site: Site,
    pub locked: bool,
}

/// RegisterGlobalVariable refuses a name it already holds,
/// so the first [Constants] declaration in load order wins, flags included.
pub fn kept_globals(context: &RuleContext) -> HashMap<String, Kept> {
    let mut kept: HashMap<String, Kept> = HashMap::new();
    for (id, file) in loaded_files(context) {
        for (index, variable) in constants_globals(file) {
            let key = global_variable_key(&variable.name, &file.namespace);
            let beats =
                |first: &Kept| first.site.file != id && loads_before(context, id, first.site.file);
            if kept.get(&key).is_none_or(beats) {
                kept.insert(
                    key,
                    Kept {
                        site: Site { file: id, index },
                        locked: variable.locked,
                    },
                );
            }
        }
    }
    kept
}

/// Where a declaration is, as seen from the file being linted.
pub fn describe_site(context: &RuleContext, site: Site) -> String {
    let files = context.workspace.files();
    let file = &files[site.file];
    let line = file.range_of(file.variables[site.index].span).start.line + 1;
    if site.file == context.file_id {
        return format!("line {line}");
    }
    format!("{}:{line}", relative_location(context.file, file))
}
