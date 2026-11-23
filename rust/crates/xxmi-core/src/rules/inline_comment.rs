//! XM115: a `;` comment after a value on the same line.

use super::{Finding, FixSpec, RuleContext};
use crate::scan::Span;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        for line in &section.lines {
            let Some(value) = line.scanned.value else {
                continue;
            };
            let Some(offset) = comment_start(value.slice(context.text)) else {
                continue;
            };

            let comment = Span {
                start: value.start + offset,
                end: value.end,
            };
            // ParseIniStream: "a semicolon appearing in the middle of a line is *NOT* a comment".
            findings.push(Finding {
                span: comment,
                message:
                    "`;` after a value doesn't start a comment: the DLL keeps it in the value."
                        .to_string(),
                fix: move_above(context.text, line.scanned.content, comment),
                severity: None,
            });
        }
    }
    findings
}

/// Offset in `value` of a `;` that opens a comment: the first one at the start or after a blank.
/// A `;` glued to other text (`a;b.dds`) is left alone.
pub(super) fn comment_start(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    (0..bytes.len()).find(|&i| bytes[i] == b';' && (i == 0 || matches!(bytes[i - 1], b' ' | b'\t')))
}

/// Replaces the line with its comment, then the code, keeping the indent.
/// A line that is only a comment after the `=` has no code to keep, so it gets no fix.
fn move_above(text: &str, content: Span, comment: Span) -> Option<FixSpec> {
    let code = text[content.start..comment.start].trim_end_matches([' ', '\t']);
    if code.ends_with('=') {
        return None;
    }

    let line_start = text[..content.start].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[line_start..content.start];
    Some(FixSpec {
        title: "Move the comment above the line".to_string(),
        span: content,
        // A plain \n keeps the edit identical for LF and CRLF files; editors normalise it on insert.
        new_text: format!("{}\n{indent}{code}", comment.slice(text)),
        also: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_needs_a_blank_before_it() {
        assert_eq!(comment_start("abc ; body"), Some(4));
        assert_eq!(comment_start("; body"), Some(0));
        assert_eq!(comment_start("abc\t;x"), Some(4));
        assert_eq!(comment_start("a;b.dds"), None);
        assert_eq!(comment_start("abc"), None);
    }
}
