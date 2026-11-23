//! XM005 and XM006: a section header with text after its `]`, or with no `]` at all.

use super::{Finding, FixSpec, RuleContext};
use crate::scan::Span;

/// Offset of the first `]` in a header, which is where `ParseIniSectionLine` ends the name.
fn closing_bracket(header: Span, text: &str) -> Option<usize> {
    header
        .slice(text)
        .find(']')
        .map(|offset| header.start + offset)
}

/// What follows the closing `]`, from the bracket on. A `;` suffix is not reported:
/// the DLL ignores everything after the `]`, so a trailing comment changes nothing.
fn suffix_after_bracket(header: Span, text: &str) -> Option<(Span, Span)> {
    let bracket = closing_bracket(header, text)?;
    let after = Span {
        start: bracket + 1,
        end: header.end,
    };

    let tail = after.slice(text);
    let trimmed = tail.trim_start_matches([' ', '\t']);
    if trimmed.is_empty() || trimmed.starts_with(';') {
        return None;
    }

    let shown = Span {
        start: header.end - trimmed.len(),
        end: header.end,
    };
    Some((shown, after))
}

/// Whether XM005 or XM006 reports this header, so XM001 leaves its line alone.
pub fn is_malformed(header: Span, text: &str) -> bool {
    closing_bracket(header, text).is_none() || suffix_after_bracket(header, text).is_some()
}

pub fn check_suffix(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let header = section.header.content;
        let Some((shown, after)) = suffix_after_bracket(header, context.text) else {
            continue;
        };

        findings.push(Finding {
            span: shown,
            message: format!(
                "Text after `]` is ignored: `{}` (the DLL reads the section name up to the first `]`).",
                shown.slice(context.text)
            ),
            fix: Some(FixSpec {
                title: "Remove the text after `]`".to_string(),
                span: after,
                new_text: String::new(),
                also: Vec::new(),
            }),
            severity: None,
        });
    }
    findings
}

pub fn check_unclosed(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let header = section.header.content;
        if closing_bracket(header, context.text).is_some() {
            continue;
        }

        findings.push(Finding {
            span: header,
            message:
                "Section header has no closing `]`: the DLL takes the rest of the line as the name."
                    .to_string(),
            fix: Some(FixSpec {
                title: "Add the closing `]`".to_string(),
                span: Span {
                    start: header.end,
                    end: header.end,
                },
                new_text: "]".to_string(),
                also: Vec::new(),
            }),
            severity: None,
        });
    }
    findings
}
