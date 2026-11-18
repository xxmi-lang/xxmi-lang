//! XM204: a variable that is declared but never read or written.

use super::{Finding, RuleContext};
use crate::model::Scope;
use crate::workspace::symbols::global_variable_key;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    // Globals in files pulled in by includes are usually a library's API for other mods.
    let check_globals = context.workspace.is_target(file) && !context.is_library();
    let used = context.workspace.usage();

    file.variables
        .iter()
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
