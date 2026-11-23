//! XM114: a TextureOverride / ShaderOverride setting written inside an `if` block.

use super::keywords::{FlowKeyword, flow_keyword};
use super::{Finding, FixSpec, RuleContext, insert_line_above, line_start_indent};
use crate::model::{LineModel, SectionModel};
use crate::scan::Span;
use crate::spec::SpecSection;

const OVERRIDE_SECTIONS: [&str; 2] = ["TextureOverride", "ShaderOverride"];

/// A setting line found in one branch of an `if` chain.
struct Setting {
    key: String,
    value: String,
    content: Span,
    /// Index of its finding.
    finding: usize,
}

struct Chain {
    if_content: Span,
    has_else: bool,
    branches: Vec<Vec<Setting>>,
}

impl Chain {
    fn new(if_content: Span) -> Self {
        Self {
            if_content,
            has_else: false,
            branches: vec![Vec::new()],
        }
    }

    fn start_else(&mut self) {
        self.has_else = true;
        self.branches.push(Vec::new());
    }
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section
            .spec
            .filter(|spec| OVERRIDE_SECTIONS.contains(&spec.name.as_str()))
        else {
            continue;
        };
        check_section(context, section, spec, &mut findings);
    }
    findings
}

fn check_section(
    context: &RuleContext,
    section: &SectionModel,
    spec: &SpecSection,
    findings: &mut Vec<Finding>,
) {
    let mut open: Vec<Chain> = Vec::new();
    for line in &section.lines {
        match (flow_keyword(line, context.text), open.last_mut()) {
            (Some(FlowKeyword::If), _) => open.push(Chain::new(line.scanned.content)),
            (Some(FlowKeyword::Elif), Some(top)) => top.branches.push(Vec::new()),
            (Some(FlowKeyword::Else), Some(top)) => top.start_else(),
            (Some(FlowKeyword::Endif), Some(_)) => close_chain(context.text, &mut open, findings),
            (None, Some(top)) => flag_setting(context, spec, line, top, findings),
            _ => {}
        }
    }
}

fn close_chain(text: &str, open: &mut Vec<Chain>, findings: &mut [Finding]) {
    if let Some(chain) = open.pop() {
        attach_hoists(text, &chain, findings);
    }
}

fn flag_setting(
    context: &RuleContext,
    spec: &SpecSection,
    line: &LineModel,
    chain: &mut Chain,
    findings: &mut Vec<Finding>,
) {
    let Some(setting) = setting_of(context, spec, line, findings.len()) else {
        return;
    };
    findings.push(Finding::new(
        line.scanned.content,
        format!(
            "`{}` inside an `if` is not conditional: the DLL reads it as a section setting, whatever the `if` says.",
            setting.key
        ),
    ));
    if let Some(branch) = chain.branches.last_mut() {
        branch.push(setting);
    }
}

/// The line as a setting of the section;
/// ParseCommandList skips these keys before it reads flow control,
/// and the GetIni* readers never look at `if` (IniHandler.cpp).
fn setting_of(
    context: &RuleContext,
    spec: &SpecSection,
    line: &LineModel,
    finding: usize,
) -> Option<Setting> {
    let key = line.scanned.key?.slice(context.text).to_lowercase();
    if !spec.keys.iter().any(|known| known.name == key) {
        return None;
    }

    let value = line.scanned.value.map_or("", |v| v.slice(context.text));
    Some(Setting {
        key,
        value: value.to_string(),
        content: line.scanned.content,
        finding,
    })
}

/// A setting that appears once, with the same value, in every branch of a chain that has an `else`
/// is set the same whichever branch runs, so one copy above the `if` replaces them all.
/// Only the first copy's finding carries the fix, so applying every fix doesn't repeat it.
fn attach_hoists(text: &str, chain: &Chain, findings: &mut [Finding]) {
    if !chain.has_else {
        return;
    }

    let Some(first_branch) = chain.branches.first() else {
        return;
    };
    for setting in first_branch {
        let copies: Option<Vec<&Setting>> = chain
            .branches
            .iter()
            .map(|branch| only_copy(branch, setting))
            .collect();
        if let Some(copies) = copies {
            findings[setting.finding].fix =
                Some(hoist_fix(text, chain.if_content, setting, &copies));
        }
    }
}

fn only_copy<'a>(branch: &'a [Setting], wanted: &Setting) -> Option<&'a Setting> {
    let mut same_key = branch.iter().filter(|s| s.key == wanted.key);
    let copy = same_key.next()?;
    (same_key.next().is_none() && copy.value == wanted.value).then_some(copy)
}

fn hoist_fix(text: &str, if_content: Span, first: &Setting, copies: &[&Setting]) -> FixSpec {
    let (span, new_text) = insert_line_above(text, if_content.start, first.content.slice(text));

    FixSpec {
        title: format!("Move `{}` above the `if`", first.key),
        span,
        new_text,
        also: copies
            .iter()
            .map(|copy| (whole_line(text, copy.content), String::new()))
            .collect(),
    }
}

/// The line with its indent and line break.
fn whole_line(text: &str, content: Span) -> Span {
    let end = text[content.end..]
        .find('\n')
        .map_or(text.len(), |i| content.end + i + 1);
    Span {
        start: line_start_indent(text, content.start).0,
        end,
    }
}
