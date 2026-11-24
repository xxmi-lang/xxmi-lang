//! XM403: a flow keyword that isn't lowercase, or a section type not spelled as in the DLL.

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
    let mut findings = flow_findings(context);
    findings.extend(section_type_findings(context));
    findings
}

/// The type prefix of each recognised header, when it isn't spelled as the spec has it.
/// Only the prefix is touched: the rest of the name is the user's,
/// and references compare it case-insensitively
/// (`WStringInsensitiveLess`, IniHandler.cpp:172), so a fix breaks none.
fn section_type_findings(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section.spec else {
            continue;
        };

        let canonical = spec.name.as_str();
        let start = section.name.start;
        let span = Span {
            start,
            end: start + canonical.len(),
        };
        let Some(written) = context.text.get(span.start..span.end) else {
            continue;
        };
        // A non-ASCII letter can lower-case into the prefix (the Kelvin sign into `k`).
        if written == canonical || !written.eq_ignore_ascii_case(canonical) {
            continue;
        }

        findings.push(Finding {
            span,
            message: format!("Section type `{written}` should be spelled `{canonical}`."),
            fix: Some(FixSpec {
                title: format!("Change to `{canonical}`"),
                span,
                new_text: canonical.to_string(),
                also: Vec::new(),
            }),
            severity: None,
        });
    }
    findings
}

fn flow_findings(context: &RuleContext) -> Vec<Finding> {
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
                    also: Vec::new(),
                }),
                severity: None,
            });
        }
    }
    findings
}
