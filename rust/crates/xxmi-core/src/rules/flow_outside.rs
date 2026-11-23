//! XM111: `if`, `elif`, `else` or `endif` where the DLL doesn't read flow control.

use super::keywords::is_misplaced_flow;
use super::{Finding, RuleContext};
use crate::model::LineModel;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let preamble = context.file.preamble.iter().map(|line| (None, line));
    let sections = context
        .file
        .sections
        .iter()
        .flat_map(|section| section.lines.iter().map(move |line| (Some(section), line)));

    preamble
        .chain(sections)
        .filter(|(section, line)| is_misplaced_flow(*section, line, context.text))
        .map(|(section, line)| finding(context, section.map(|s| context.file.slice(s.name)), line))
        .collect()
}

fn finding(context: &RuleContext, section: Option<&str>, line: &LineModel) -> Finding {
    let content = line.scanned.content;
    let place = match section {
        Some(name) => format!("[{name}]"),
        None => "the preamble".to_string(),
    };
    Finding::new(
        content,
        format!(
            "Flow control outside a command list: `{}` in {place} is never parsed as `if`/`endif`.",
            content.slice(context.text)
        ),
    )
}
