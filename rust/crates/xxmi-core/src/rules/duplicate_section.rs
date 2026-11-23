//! XM107: a section name the DLL has already seen in an earlier file (it keeps the first).

use std::collections::HashMap;

use super::{Finding, RuleContext};

pub fn check(context: &RuleContext) -> Vec<Finding> {
    // Files the DLL never loads (`exclude_recursive`, e.g. DISABLED folders) can only clash with
    // themselves: a disabled copy of a library isn't a duplicate of the enabled one.
    let workspace = context.workspace;
    let excluded = workspace.is_excluded(&context.file.path, None);
    let own_package = context.package_root;

    let mut first_by_name: HashMap<String, (usize, u32)> = HashMap::new();
    let mut findings = Vec::new();
    for (id, other) in workspace.files().iter().enumerate() {
        let is_this = id == context.file_id;
        if !is_this && (excluded || workspace.is_excluded(&other.path, None)) {
            continue;
        }
        // One lint run can span several packages; the DLL only ever loads one.
        if workspace.package_root(other) != own_package {
            continue;
        }

        for section in &other.sections {
            let name = other.slice(section.name);
            let prefixed = context.lookup.section_prefix(name).is_some();
            // Global sections from included files merge into the main one instead of duplicating.
            if !other.namespace.is_empty() && !prefixed {
                continue;
            }

            let key = section.qualified_name.to_lowercase();
            let Some(&(first_id, first_line)) = first_by_name.get(&key) else {
                first_by_name.insert(key, (id, section.header.line));
                continue;
            };
            if is_this {
                let location = if first_id == id {
                    format!("line {}", first_line + 1)
                } else {
                    format!(
                        "{}:{}",
                        workspace.files()[first_id].path.display(),
                        first_line + 1
                    )
                };
                findings.push(Finding::new(
                    section.name,
                    format!(
                        "Duplicate section found: [{name}] (first at {location}; the DLL uses the first)"
                    ),
                ));
            }
        }
    }
    findings
}
