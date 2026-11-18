//! XM001: parse error (tree-sitter ERROR/MISSING node), except on expression lines, which the
//! TypeScript core reports as XM109.

use super::{Finding, RuleContext};
use crate::model::{Issue, LineModel};

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let model = context.model;
    let preamble = model
        .preamble_issues
        .iter()
        .map(|issue| (issue, &model.preamble));
    let sections = model.sections.iter().flat_map(|section| {
        section
            .issues
            .iter()
            .map(move |issue| (issue, &section.lines))
    });

    preamble
        .chain(sections)
        .filter(|(issue, lines)| !is_expression_line(line_of(lines, issue), context.text))
        .map(|(issue, _)| Finding {
            span: issue.span,
            message: issue.message.clone(),
            fix: None,
        })
        .collect()
}

fn line_of<'a>(lines: &'a [LineModel], issue: &Issue) -> Option<&'a LineModel> {
    lines.iter().find(|line| line.scanned.line == issue.line)
}

/// Lines whose value the DLL parses as an expression (`CommandListExpression::parse`).
fn is_expression_line(line: Option<&LineModel>, text: &str) -> bool {
    let Some(line) = line else {
        return false;
    };
    let content = line.scanned.content.slice(text).to_lowercase();

    starts_with_keyword(&content, &["if", "elif", "else if"])
        || starts_with_variable(&content)
        || content.starts_with("local ")
}

/// `^(if|elif|else if)\s`
fn starts_with_keyword(content: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| {
        content
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
    })
}

/// `^(local\s+)?\$`
fn starts_with_variable(content: &str) -> bool {
    let rest = match content.strip_prefix("local") {
        Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
        _ => content,
    };
    rest.starts_with('$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_prefixes() {
        assert!(starts_with_keyword("if $a == 1", &["if"]));
        assert!(starts_with_keyword("else if x", &["if", "elif", "else if"]));
        assert!(!starts_with_keyword("iffy = 1", &["if"]));
        assert!(starts_with_variable("$a = 1"));
        assert!(starts_with_variable("local $a = 1"));
        assert!(!starts_with_variable("local a = 1"));
    }
}
