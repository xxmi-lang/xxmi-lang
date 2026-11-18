//! XM206: `CommandList\Ns\Name` or `$\Ns\name` where no loaded file declares the namespace.

use super::{Finding, RuleContext};
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
        findings.push(Finding::new(
            reference.span,
            format!(
                "Unknown namespace `{namespace}` in `{}`: no loaded file declares `namespace = {namespace}`",
                reference.text
            ),
        ));
    }
    findings
}
