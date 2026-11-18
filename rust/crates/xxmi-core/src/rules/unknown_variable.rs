//! XM203: a `$variable` that no `global` or `local` declares.

use super::{Finding, RuleContext};
use crate::model::SymbolKind;
use crate::workspace::symbols::explicit_namespace;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let symbols = context.symbols();

    context
        .file
        .references
        .iter()
        .filter(|r| r.kind == SymbolKind::Variable)
        .filter(|r| symbols.resolve_variable(r, context.file).is_none())
        // XM206 reports a namespace nobody declares.
        .filter(|r| {
            let namespace = explicit_namespace(&r.text, context.lookup);
            !namespace.is_some_and(|ns| !symbols.has_namespace(&ns))
        })
        .map(|r| {
            Finding::new(
                r.span,
                format!(
                    "Unknown variable: {} (declare it with `global` in [Constants] or `local`)",
                    r.text
                ),
            )
        })
        .collect()
}
