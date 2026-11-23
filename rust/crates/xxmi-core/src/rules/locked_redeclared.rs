//! XM211: a `global locked` declared again, which the DLL ignores.

use std::collections::HashSet;

use super::global_redeclared::constants_globals;
use super::kept_globals::{Site, describe_site, kept_globals};
use super::{Finding, RuleContext};
use crate::workspace::symbols::global_variable_key;

/// A later declaration of a locked global, as an index into `IniFile::variables`, and the one kept.
struct LockedRedeclaration {
    index: usize,
    kept: Site,
}

/// ParseConstantsSection (IniHandler.cpp): the first declaration wins, locked flag included,
/// so a redeclaration's value and modifiers never apply.
fn locked_redeclarations(context: &RuleContext) -> Vec<LockedRedeclaration> {
    let file = context.file;
    if constants_globals(file).next().is_none() {
        return Vec::new();
    }

    let kept = kept_globals(context);
    constants_globals(file)
        .filter_map(|(index, variable)| {
            let first = kept.get(&global_variable_key(&variable.name, &file.namespace))?;
            let here = Site {
                file: context.file_id,
                index,
            };
            (first.locked && first.site != here).then_some(LockedRedeclaration {
                index,
                kept: first.site,
            })
        })
        .collect()
}

/// The declarations this rule reports, for XM209 to leave alone.
pub fn locked_redeclared_indices(context: &RuleContext) -> HashSet<usize> {
    locked_redeclarations(context)
        .into_iter()
        .map(|r| r.index)
        .collect()
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    locked_redeclarations(context)
        .into_iter()
        .map(|r| {
            let variable = &context.file.variables[r.index];
            Finding::new(
                variable.span,
                format!(
                    "Global {} is declared `locked` at {}; the DLL keeps that declaration and ignores this one, so its value never applies.",
                    variable.name,
                    describe_site(context, r.kept)
                ),
            )
        })
        .collect()
}
