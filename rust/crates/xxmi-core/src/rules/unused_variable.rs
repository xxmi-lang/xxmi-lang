//! XM204: a variable that is declared but never read or written.

use std::collections::HashSet;

use super::global_redeclared::redeclared_globals;
use super::local_redeclared::redeclared_locals;
use super::{Finding, RuleContext};
use crate::model::Scope;
use crate::workspace::symbols::global_variable_key;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    // Globals in files pulled in by includes are usually a library's API for other mods.
    let check_globals = context.workspace.is_target(file) && !context.is_library();
    let used = context.workspace.usage_for(file);
    // XM209 and XM210 own the declarations the DLL drops.
    let dropped: HashSet<usize> = redeclared_globals(file)
        .into_iter()
        .chain(redeclared_locals(file, context.text))
        .map(|r| r.index)
        .collect();

    file.variables
        .iter()
        .enumerate()
        .filter(|(index, _)| !dropped.contains(index))
        .map(|(_, v)| v)
        .filter(|v| match v.scope {
            Scope::Local => !used.locals.contains(&(context.file_id, v.span.start)),
            Scope::Global => {
                check_globals
                    && !used
                        .globals
                        .contains(&global_variable_key(&v.name, &file.namespace))
            }
        })
        .map(|v| {
            let scope = if v.scope == Scope::Local {
                "Local"
            } else {
                "Global"
            };
            Finding::new(v.span, format!("{scope} variable {} is never used", v.name))
        })
        .collect()
}
