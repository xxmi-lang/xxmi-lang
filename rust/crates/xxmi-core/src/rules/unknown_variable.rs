//! XM203: a `$variable` that no `global` or `local` declares.
//!
//! The DLL drops the line: `CommandArgumentReader::GetVariable` fails for an assignment target and
//! `CommandListOperand::parse_variable` fails inside an expression (`if` conditions included), and
//! either leaves `ParseCommandList` to log "Unrecognised entry" (CommandList.cpp, IniHandler.cpp).

use super::declare_global::declare_global;
use super::{Finding, FixSpec, RuleContext, replace_fix};
use crate::model::{Reference, Scope, SymbolKind};
use crate::suggest::did_you_mean;
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
            // A name with a namespace belongs to another file's declarations, so only a bare one is fixed here.
            let own = explicit_namespace(&r.text, context.lookup).is_none();
            let names = if own {
                declared_names(context, r)
            } else {
                Vec::new()
            };
            let candidates: Vec<&str> = names.iter().map(String::as_str).collect();
            let hint = did_you_mean(&r.text, candidates.iter().copied());
            let sentence = if hint.is_empty() { "" } else { "." };

            Finding {
                span: r.span,
                message: format!(
                    "Unknown variable: {} (declare it with `global` in [Constants] or `local`){sentence}{hint}",
                    r.text
                ),
                fix: own.then(|| fix_for(context, r, &candidates)).flatten(),
                severity: None,
            }
        })
        .collect()
}

/// A rename when a declared name is close, else a declaration, which would only hide a typo.
fn fix_for(context: &RuleContext, reference: &Reference, candidates: &[&str]) -> Option<FixSpec> {
    if let Some(rename) = replace_fix(reference.span, &reference.text, candidates, "") {
        return Some(rename);
    }

    // A local of that name declared out of reach (a sibling branch, or later) would be masked.
    let lower = reference.text.to_lowercase();
    let local_elsewhere = context
        .file
        .variables
        .iter()
        .any(|v| v.scope == Scope::Local && v.name.to_lowercase() == lower);
    (!local_elsewhere).then(|| declare_global(context, &reference.text))
}

/// Locals of the reference's section and the globals this file reaches without a namespace.
fn declared_names(context: &RuleContext, reference: &Reference) -> Vec<String> {
    let file = context.file;
    let own = format!("$\\{}\\", file.namespace.to_lowercase());

    let locals = file
        .variables
        .iter()
        .filter(|v| v.scope == Scope::Local && v.section == reference.section)
        .map(|v| v.name.clone());
    let globals = context.symbols().all_variables().filter_map(|v| {
        if v.key.contains('\\') {
            let bare = (!file.namespace.is_empty()).then(|| v.key.strip_prefix(&own))??;
            return Some(format!("${bare}"));
        }
        Some(v.key.clone())
    });
    locals.chain(globals).collect()
}
