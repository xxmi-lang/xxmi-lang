//! XM205: a command list, resource or custom shader nothing refers to.

use super::{Finding, RuleContext};
use crate::model::SymbolKind;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    // Library files and d3dx.ini (no namespace: its sections are global) are called from mods.
    if !context.workspace.is_target(file) || context.is_library() || file.namespace.is_empty() {
        return Vec::new();
    }

    let used = &context.workspace.usage_for(file).sections;
    file.sections
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                Some(SymbolKind::CommandList | SymbolKind::Resource | SymbolKind::CustomShader)
            )
        })
        .filter(|s| !used.contains(&s.qualified_name.to_lowercase()))
        .map(|s| {
            Finding::new(
                s.name,
                format!("[{}] is never referenced", file.slice(s.name)),
            )
        })
        .collect()
}
