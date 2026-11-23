//! XM003: a line in the preamble (before the first section) that the DLL doesn't read.

use super::keywords::is_misplaced_flow;
use super::{Finding, RuleContext, comment_out_fix};
use crate::model::LineModel;

/// `ParseIniPreamble` honours only `namespace` and `condition`, case-insensitively.
pub fn is_honoured(line: &LineModel, text: &str) -> bool {
    let Some(key) = line.scanned.key else {
        return false;
    };
    let key = key.slice(text);
    key.eq_ignore_ascii_case("namespace") || key.eq_ignore_ascii_case("condition")
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for line in &context.file.preamble {
        // XM111 owns flow keywords, wherever they stray.
        if is_honoured(line, context.text) || is_misplaced_flow(None, line, context.text) {
            continue;
        }

        let content = line.scanned.content;
        findings.push(Finding {
            span: content,
            message: format!(
                "Entry outside of section: {}. Only `namespace` and `condition` are read before the first section; the DLL warns and ignores the line.",
                content.slice(context.text)
            ),
            fix: Some(comment_out_fix(content)),
            severity: None,
        });
    }
    findings
}
