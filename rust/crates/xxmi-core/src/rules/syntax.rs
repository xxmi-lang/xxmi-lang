//! XM001, XM002 and XM109: what tree-sitter and the decoder could not read. An issue on an
//! expression line is XM109, anywhere else XM001.

use super::keywords::{is_expression_line, is_misplaced_flow};
use super::{Finding, RuleContext, bare_line, header, outside_section};
use crate::encoding::Encoding;
use crate::model::{Issue, LineModel, SectionModel};
use crate::scan::Span;

const NON_UTF8: &str = "File isn't valid UTF-8 (likely GBK or Shift-JIS). The DLL reads inis as UTF-8, so non-ASCII text such as names and paths won't read as intended.";

/// Every syntax issue with the line it sits on, preamble first,
/// minus the lines a dedicated rule names more precisely.
fn issues<'a>(context: &'a RuleContext) -> Vec<(&'a Issue, Option<&'a LineModel>)> {
    let file = context.file;
    let preamble = file
        .preamble_issues
        .iter()
        .map(|issue| (issue, line_of(&file.preamble, issue)))
        // XM003 owns every preamble line but `namespace` and `condition`, XM111 the flow keywords.
        .filter(|(_, line)| {
            line.is_none_or(|line| outside_section::is_honoured(line, context.text))
        });
    let sections = file.sections.iter().flat_map(|section| {
        section
            .issues
            .iter()
            .map(move |issue| (issue, line_of(&section.lines, issue), section))
            .filter(|(issue, line, section)| !is_claimed(context, section, issue, *line))
            .map(|(issue, line, _)| (issue, line))
    });
    preamble.chain(sections).collect()
}

/// Whether XM005, XM006, XM103, XM110 or XM111 reports the line the issue starts on.
fn is_claimed(
    context: &RuleContext,
    section: &SectionModel,
    issue: &Issue,
    line: Option<&LineModel>,
) -> bool {
    let Some(line) = line else {
        return issue.line == section.header.line
            && header::is_malformed(section.header.content, context.text);
    };
    // XM110 owns `key =`: it names the cause the syntax error only hints at.
    has_empty_value(Some(line))
        || bare_line::is_malformed(section, line, context.text)
        || is_misplaced_flow(Some(section), line, context.text)
}

fn line_of<'a>(lines: &'a [LineModel], issue: &Issue) -> Option<&'a LineModel> {
    lines.iter().find(|line| line.scanned.line == issue.line)
}

fn has_empty_value(line: Option<&LineModel>) -> bool {
    line.and_then(|line| line.scanned.value)
        .is_some_and(|value| value.start == value.end)
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
