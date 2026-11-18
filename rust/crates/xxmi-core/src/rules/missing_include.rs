//! XM207: an `include =` that names no file.

use std::path::Path;

use super::{Finding, RuleContext};

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    let resolver = context.workspace.resolver();
    let dir = file.path.parent().unwrap_or(Path::new(""));

    let mut findings = Vec::new();
    for section in file
        .sections
        .iter()
        .filter(|s| s.spec.is_some_and(|s| s.name == "Include"))
    {
        for line in &section.lines {
            let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            let written = file.slice(value);
            // Only `include`: ParseNamespacedIniFile warns "Error opening" for a missing file, while
            // a missing `include_recursive` folder is only logged at info level.
            if file.slice(key).to_lowercase() != "include" || written.is_empty() {
                continue;
            }

            let found = resolver.resolve(dir, written);
            if found.is_none_or(|path| resolver.is_directory(&path)) {
                findings.push(Finding::new(
                    value,
                    format!("Error opening include file: {written} (not found)"),
                ));
            }
        }
    }
    findings
}
