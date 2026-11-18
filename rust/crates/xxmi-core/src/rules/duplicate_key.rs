//! XM104: a key the DLL reads once but the section sets twice (it uses the first).

use std::collections::HashMap;

use super::{Finding, RuleContext};
use crate::spec::SectionKind;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section.spec else {
            continue;
        };
        // Global sections in included files may override the main d3dx.ini's values.
        let prefixed = context
            .lookup
            .section_prefix(context.file.slice(section.name))
            .is_some();
        if !context.file.namespace.is_empty() && !prefixed {
            continue;
        }

        let command_list = spec.kind == SectionKind::CommandList;
        let mut seen: HashMap<String, u32> = HashMap::new();
        for line in &section.lines {
            let Some(key_span) = line.scanned.key else {
                continue;
            };
            let key_text = key_span.slice(context.text);
            let name = if command_list {
                line.command.clone().unwrap_or_default()
            } else {
                key_text.to_lowercase()
            };

            let key = context.lookup.key(spec, &name);
            // Only keys parsed outside the command list are checked ("non-command list key").
            if command_list && key.is_none() || !command_list && spec.allows_duplicate_keys {
                continue;
            }
            if key.is_some_and(|k| k.repeatable) {
                continue;
            }

            match seen.get(&name) {
                Some(first) => {
                    let what = if command_list {
                        "Duplicate non-command list key found"
                    } else {
                        "Duplicate key found"
                    };
                    findings.push(Finding::new(
                        key_span,
                        format!(
                            "{what}: {key_text} (first on line {}; the DLL uses the first)",
                            first + 1
                        ),
                    ));
                }
                None => {
                    seen.insert(name, line.scanned.line);
                }
            }
        }
    }
    findings
}
