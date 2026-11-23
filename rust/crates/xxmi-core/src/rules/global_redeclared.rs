//! XM209: a global declared twice in [Constants].

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use super::locked_redeclared::locked_redeclared_indices;
use super::{Finding, RuleContext};
use crate::model::{IniFile, Scope, VariableDeclaration};
use crate::workspace::symbols::global_variable_key;

/// A declaration the DLL drops, and the earlier one it keeps; both index `IniFile::variables`.
pub struct Redeclaration {
    pub index: usize,
    pub first: usize,
}

/// ParseConstantsSection only registers globals from [Constants].
pub fn constants_globals(file: &IniFile) -> impl Iterator<Item = (usize, &VariableDeclaration)> {
    file.variables.iter().enumerate().filter(|(_, variable)| {
        let in_constants = variable
            .section
            .and_then(|section| file.sections[section].spec)
            .is_some_and(|spec| spec.name == "Constants");
        variable.scope == Scope::Global && in_constants
    })
}

/// RegisterGlobalVariable refuses a name it already holds,
/// so the first declaration wins and the DLL warns "Redeclaration".
pub fn redeclared_globals(file: &IniFile) -> Vec<Redeclaration> {
    let mut first_by_key: HashMap<String, usize> = HashMap::new();
    let mut redeclared = Vec::new();

    for (index, variable) in constants_globals(file) {
        match first_by_key.entry(global_variable_key(&variable.name, &file.namespace)) {
            Entry::Occupied(first) => redeclared.push(Redeclaration {
                index,
                first: *first.get(),
            }),
            Entry::Vacant(slot) => {
                slot.insert(index);
            }
        }
    }
    redeclared
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    let redeclared = redeclared_globals(file);
    // XM211 owns the redeclarations of a locked global.
    let locked = if redeclared.is_empty() {
        HashSet::new()
    } else {
        locked_redeclared_indices(context)
    };

    redeclared
        .into_iter()
        .filter(|r| !locked.contains(&r.index))
        .map(|r| {
            let variable = &file.variables[r.index];
            let first_line = file.range_of(file.variables[r.first].span).start.line + 1;
            Finding::new(
                variable.span,
                format!(
                    "Global {} is already declared on line {first_line}; the DLL keeps the first and ignores this one.",
                    variable.name
                ),
            )
        })
        .collect()
}
