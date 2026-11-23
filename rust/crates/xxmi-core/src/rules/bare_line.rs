//! XM103: a line without `=` in a section that only reads keys.

use super::keywords::is_misplaced_flow;
use super::{Finding, RuleContext, comment_out_fix};
use crate::model::{LineModel, SectionModel};

/// `ParseIniKeyValLine` warns "Malformed line" and drops it. XM111 owns a stray `else` or `endif`.
pub fn is_malformed(section: &SectionModel, line: &LineModel, text: &str) -> bool {
    let strict = section.spec.is_some_and(|spec| !spec.allows_bare_lines);
    strict && line.scanned.key.is_none() && !is_misplaced_flow(Some(section), line, text)
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        for line in &section.lines {
            if !is_malformed(section, line, context.text) {
                continue;
            }

            let content = line.scanned.content;
            findings.push(Finding {
                span: content,
                message: format!("Malformed line: \"{}\"", content.slice(context.text)),
                fix: Some(comment_out_fix(content)),
                severity: None,
            });
        }
    }
    findings
}
