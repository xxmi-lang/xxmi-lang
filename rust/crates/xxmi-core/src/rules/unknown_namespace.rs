//! XM206: `CommandList\Ns\Name` or `$\Ns\name` where no loaded file declares the namespace.

use super::{Finding, RuleContext, Severity};
use crate::model::SymbolKind;
use crate::workspace::symbols::explicit_namespace;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let symbols = context.symbols();

    let mut findings = Vec::new();
    for reference in &context.file.references {
        let Some(namespace) = explicit_namespace(&reference.text, context.lookup) else {
            continue;
        };
        if symbols.has_namespace(&namespace) {
            continue;
        }
        let mut finding = Finding::new(
            reference.span,
            format!(
                "Unknown namespace `{namespace}` in `{}`: no loaded file declares `namespace = {namespace}`",
                reference.text
            ),
        );
        // A variable's file may not be loaded; a missing command list or resource is surely broken.
        if reference.kind == SymbolKind::Variable {
            finding.severity = Some(Severity::Warning);
        }
        findings.push(finding);
    }
    findings
}
