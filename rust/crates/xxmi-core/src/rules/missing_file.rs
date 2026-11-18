//! XM208: a resource `filename =` that names no file.

use std::path::Path;

use super::{Finding, RuleContext};

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    let resolver = context.workspace.resolver();
    let dir = file.path.parent().unwrap_or(Path::new(""));

    let mut findings = Vec::new();
    for section in file.sections.iter().filter(|s| {
        s.spec
            .is_some_and(|s| s.name == "Resource" || s.name == "Pool")
    }) {
        for line in &section.lines {
            let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            let written = file.slice(value);
            if file.slice(key).to_lowercase() != "filename" || written.is_empty() {
                continue;
            }
            // An absolute Windows path can't be checked on this machine.
            if is_windows_absolute(written) {
                continue;
            }

            // ParseResourceSection: relative to the declaring ini's folder, then the 3DMigoto folder.
            let found = resolver.resolve(dir, written).or_else(|| {
                context
                    .package_root
                    .and_then(|root| resolver.resolve(root, written))
            });
            if found.is_none_or(|path| resolver.is_directory(&path)) {
                findings.push(Finding::new(value, format!("File not found: {written}")));
            }
        }
    }
    findings
}

/// `^[a-z]:[\\/]`
fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}
