//! XM001, XM002 and XM109: what tree-sitter and the decoder could not read. An issue on an
//! expression line is XM109, anywhere else XM001.

use super::keywords::is_expression_line;
use super::{Finding, RuleContext};
use crate::encoding::Encoding;
use crate::model::{Issue, LineModel};
use crate::scan::Span;

const NON_UTF8: &str = "File isn't valid UTF-8 (likely GBK or Shift-JIS). The DLL reads inis as UTF-8, so non-ASCII text such as names and paths won't read as intended.";

/// Every syntax issue with the line it sits on, preamble first.
fn issues<'a>(context: &'a RuleContext) -> Vec<(&'a Issue, Option<&'a LineModel>)> {
    let file = context.file;
    let preamble = file
        .preamble_issues
        .iter()
        .map(|issue| (issue, line_of(&file.preamble, issue)));
    let sections = file.sections.iter().flat_map(|section| {
        section
            .issues
            .iter()
            .map(move |issue| (issue, line_of(&section.lines, issue)))
    });
    preamble.chain(sections).collect()
}

fn line_of<'a>(lines: &'a [LineModel], issue: &Issue) -> Option<&'a LineModel> {
    lines.iter().find(|line| line.scanned.line == issue.line)
}

pub fn check_parse_errors(context: &RuleContext) -> Vec<Finding> {
    issues(context)
        .into_iter()
        .filter(|(_, line)| !is_expression_line(*line, context.text))
        .map(|(issue, _)| Finding::new(issue.span, issue.message.clone()))
        .collect()
}

pub fn check_expressions(context: &RuleContext) -> Vec<Finding> {
    issues(context)
        .into_iter()
        .filter(|(_, line)| is_expression_line(*line, context.text))
        .map(|(issue, _)| {
            let detail = issue.message.strip_prefix("Syntax error: ");
            Finding::new(
                issue.span,
                format!("Malformed expression: {}", detail.unwrap_or(&issue.message)),
            )
        })
        .collect()
}

pub fn check_encoding(context: &RuleContext) -> Vec<Finding> {
    if context.file.encoding != Encoding::Unknown {
        return Vec::new();
    }
    vec![Finding::new(
        Span { start: 0, end: 0 },
        NON_UTF8.to_string(),
    )]
}
