//! XM106: `if`/`elif`/`else`/`endif` that don't pair up in a command list.

use super::keywords::{FlowKeyword, flow_keyword};
use super::{Finding, RuleContext};
use crate::scan::Span;
use crate::spec::SectionKind;

struct OpenIf {
    content: Span,
    saw_else: bool,
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        if section.spec.map(|s| s.kind) != Some(SectionKind::CommandList) {
            continue;
        }

        let mut open: Vec<OpenIf> = Vec::new();
        for line in &section.lines {
            let Some(keyword) = flow_keyword(line, context.text) else {
                continue;
            };
            let content = line.scanned.content;
            let text = content.slice(context.text);

            match (keyword, open.last_mut()) {
                (FlowKeyword::If, _) => open.push(OpenIf {
                    content,
                    saw_else: false,
                }),
                (FlowKeyword::Endif, Some(_)) => {
                    open.pop();
                }
                (FlowKeyword::Endif, None) => findings.push(Finding::new(
                    content,
                    "`endif` without a matching `if`".to_string(),
                )),
                (_, None) => {
                    let first = text.split(char::is_whitespace).next().unwrap_or("");
                    findings.push(Finding::new(
                        content,
                        format!("`{first}` without a matching `if`"),
                    ));
                }
                (_, Some(top)) if top.saw_else => findings.push(Finding::new(
                    content,
                    format!("`{}` after `else` in the same `if` block", keyword.as_str()),
                )),
                (FlowKeyword::Else, Some(top)) => top.saw_else = true,
                (_, Some(_)) => {}
            }
        }

        let name = context.file.slice(section.name);
        for unclosed in open {
            findings.push(Finding::new(
                unclosed.content,
                format!("Scope unbalanced: `if` without `endif` in [{name}]"),
            ));
        }
    }
    findings
}
