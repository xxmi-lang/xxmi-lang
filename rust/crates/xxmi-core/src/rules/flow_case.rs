//! XM403: a flow keyword that isn't lowercase.

use super::keywords::{FlowKeyword, flow_keyword};
use super::{Finding, FixSpec, RuleContext};
use crate::scan::Span;
use crate::spec::SectionKind;

/// Length of the keyword at the start of a flow line (`else if` counts as one keyword).
fn keyword_length(keyword: FlowKeyword, lowered: &str) -> usize {
    match keyword {
        FlowKeyword::Elif if lowered.starts_with("else if") => "else if".len(),
        _ => keyword.as_str().len(),
    }
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        if section.spec.map(|s| s.kind) != Some(SectionKind::CommandList) {
            continue;
        }

        for line in &section.lines {
            let Some(keyword) = flow_keyword(line, context.text) else {
                continue;
            };
            let content = line.scanned.content;
            let lowered = content.slice(context.text).to_lowercase();
            let span = Span {
                start: content.start,
                end: content.start + keyword_length(keyword, &lowered),
            };

            let written = span.slice(context.text);
            let lower = &lowered[..written.len()];
            if written == lower {
                continue;
            }
            // ParseCommandList lower-cases the whole line before ParseCommandListFlowControl.
            findings.push(Finding {
                span,
                message: format!(
                    "Flow keyword `{written}` should be lowercase; the DLL accepts any case."
                ),
                fix: Some(FixSpec {
                    title: format!("Change to `{lower}`"),
                    span,
                    new_text: lower.to_string(),
                }),
                severity: None,
            });
        }
    }
    findings
}
