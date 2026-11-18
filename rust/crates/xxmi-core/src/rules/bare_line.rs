//! XM103: a line without `=` in a section that only reads keys.

use super::{Finding, RuleContext};

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section.spec else {
            continue;
        };
        if spec.allows_bare_lines {
            continue;
        }

        for line in section.lines.iter().filter(|l| l.scanned.key.is_none()) {
            let content = line.scanned.content;
            findings.push(Finding::new(
                content,
                format!("Malformed line: \"{}\"", content.slice(context.text)),
            ));
        }
    }
    findings
}
