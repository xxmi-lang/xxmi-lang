//! XM110: `key =` with nothing after the `=`.

use super::{Finding, FixSpec, RuleContext};
use crate::scan::Span;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        for line in &section.lines {
            let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            if value.start != value.end {
                continue;
            }

            // ParseIniKeyValLine warns "No value found" and drops the line, so the key is never set.
            let content = line.scanned.content;
            findings.push(Finding {
                span: content,
                message: format!(
                    "No value for `{}`; the DLL warns and ignores the line.",
                    key.slice(context.text)
                ),
                fix: Some(FixSpec {
                    title: "Comment out the line".to_string(),
                    span: Span {
                        start: content.start,
                        end: content.start,
                    },
                    new_text: "; ".to_string(),
                }),
                severity: None,
            });
        }
    }
    findings
}
