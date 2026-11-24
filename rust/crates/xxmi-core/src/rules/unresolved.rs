//! XM201 and XM202: a section reference that resolves to nothing.

use super::{Finding, RuleContext, replace_fix};
use crate::defaults::is_library_global;
use crate::model::{IniFile, Reference, SymbolKind};
use crate::suggest::did_you_mean;
use crate::workspace::symbols::{SymbolTable, explicit_namespace};

pub fn check_commands(context: &RuleContext) -> Vec<Finding> {
    check_kinds(
        context,
        &[SymbolKind::CommandList, SymbolKind::CustomShader],
    )
}

pub fn check_resources(context: &RuleContext) -> Vec<Finding> {
    check_kinds(context, &[SymbolKind::Resource, SymbolKind::Pool])
}

fn check_kinds(context: &RuleContext, kinds: &[SymbolKind]) -> Vec<Finding> {
    let file = context.file;
    let symbols = context.symbols();

    let mut findings = Vec::new();
    for reference in file.references.iter().filter(|r| kinds.contains(&r.kind)) {
        if symbols
            .resolve_section(reference.kind, &reference.text, &file.namespace)
            .is_some()
        {
            continue;
        }
        // XM206 reports a namespace nobody declares.
        let namespace = explicit_namespace(&reference.text, context.lookup);
        match namespace {
            Some(ns) if !symbols.has_namespace(&ns) => continue,
            Some(_) => {}
            None if is_fallback_global(context, reference) => continue,
            None => {}
        }

        let names = local_names(symbols, reference.kind, file);
        let candidates: Vec<&str> = names.iter().map(String::as_str).collect();
        findings.push(Finding {
            span: reference.span,
            message: format!(
                "Unresolved {} `{}`.{}",
                describe(reference),
                reference.text,
                did_you_mean(&reference.text, candidates.iter().copied())
            ),
            fix: replace_fix(reference.span, &reference.text, &candidates, ""),
            severity: None,
        });
    }
    findings
}

/// Older mods call library sections that the library defines without a namespace
/// (`CommandListSkinTexture`). With no MI to say which library applies, that is no error.
fn is_fallback_global(context: &RuleContext, reference: &Reference) -> bool {
    context.workspace.profile_is_fallback(context.file)
        && is_library_global(reference.kind, &reference.text)
}

/// Names of symbols of `kind` reachable from `file` without a namespace, for suggestions.
fn local_names(symbols: &SymbolTable, kind: SymbolKind, file: &IniFile) -> Vec<String> {
    let own = format!("\\{}\\", file.namespace.to_lowercase());
    symbols
        .all_sections()
        .filter(|s| s.kind == kind)
        .map(|s| {
            let in_own =
                !file.namespace.is_empty() && s.qualified_name.to_lowercase().contains(&own);
            if in_own {
                strip_namespace(&s.qualified_name)
            } else {
                s.qualified_name.clone()
            }
        })
        .collect()
}

/// `name.replace(/\\.*\\/, '')`: the first backslash through the last one.
fn strip_namespace(name: &str) -> String {
    match (name.find('\\'), name.rfind('\\')) {
        (Some(first), Some(last)) if first < last => {
            format!("{}{}", &name[..first], &name[last + 1..])
        }
        _ => name.to_string(),
    }
}

fn describe(reference: &Reference) -> &'static str {
    match reference.kind {
        SymbolKind::CommandList => "command list",
        SymbolKind::CustomShader => "custom shader",
        SymbolKind::Pool => "resource pool",
        other => other.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_segment_is_removed_greedily() {
        assert_eq!(strip_namespace("CommandList\\ZZMI\\Foo"), "CommandListFoo");
        assert_eq!(strip_namespace("CommandList\\A\\B\\Foo"), "CommandListFoo");
        assert_eq!(strip_namespace("CommandListFoo"), "CommandListFoo");
    }
}
