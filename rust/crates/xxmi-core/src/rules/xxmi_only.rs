//! XM307: syntax the GIMI DLL (XXMI-Libs-Package v0.1.0) lacks. The spec stamps those entries
//! `gimi: false`; the rule only runs for the `gimi` dialect, so XXMI users aren't warned.

use std::collections::HashSet;

use super::xxmi_only_usage;
use super::{Finding, RuleContext};
use crate::diagnostic::Severity;
use crate::model::{LineModel, SectionModel};
use crate::profile::Dialect;
use crate::scan::Span;
use crate::spec::{SectionKind, SpecSection};

/// Sections that configure the DLL itself (d3dx.ini): a mod rarely carries them, and the GIMI
/// DLL has no use for the setting, so it is a hint rather than a warning.
const DLL_SETTINGS_SECTIONS: [&str; 5] = ["Logging", "System", "Device", "Rendering", "Hunting"];

/// What the GIMI DLL does with the line, section or expression it can't read.
#[derive(Clone, Copy)]
pub(super) enum Effect {
    Section,
    Line,
    /// An unknown name or operator in an expression fails the line it sits on.
    Expression,
}

impl Effect {
    fn describe(self) -> &'static str {
        match self {
            Self::Section => "the DLL warns and ignores the whole section",
            Self::Line => "the DLL ignores the line",
            Self::Expression => "the whole line fails to parse",
        }
    }
}

pub(super) fn unavailable(what: &str, effect: Effect) -> String {
    format!(
        "{what} isn't in the GIMI DLL: {}. Needs the XXMI DLL.",
        effect.describe()
    )
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    if context.profile.dialect != Dialect::Gimi {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let mut ignored = HashSet::new();
    for (index, section) in context.file.sections.iter().enumerate() {
        let Some(spec) = section.spec else {
            continue;
        };

        if !spec.meta.gimi {
            ignored.insert(index);
            findings.push(section_finding(context, section));
            continue;
        }
        for line in &section.lines {
            findings.extend(check_line(context, spec, line));
        }
    }

    findings.extend(xxmi_only_usage::check(context, &ignored));
    findings
}

fn section_finding(context: &RuleContext, section: &SectionModel) -> Finding {
    let name = section.name.slice(context.text);
    Finding::new(
        section.name,
        unavailable(&format!("[{name}]"), Effect::Section),
    )
}

fn check_line(
    context: &RuleContext,
    spec: &'static SpecSection,
    line: &LineModel,
) -> Option<Finding> {
    let key_span = line.scanned.key?;
    let is_command_list = spec.kind == SectionKind::CommandList;
    let name = match &line.command {
        Some(command) if is_command_list => command.as_str(),
        _ => key_span.slice(context.text),
    };

    let missing_key = context.lookup.key(spec, name).map(|key| !key.meta.gimi);
    let missing_command = || {
        is_command_list
            && context
                .lookup
                .command(name)
                .is_some_and(|command| !command.meta.gimi)
    };

    let missing = match missing_key {
        Some(missing) => missing,
        None => missing_command(),
    };
    missing.then(|| line_finding(context, spec, key_span))
}

fn line_finding(context: &RuleContext, spec: &SpecSection, key: Span) -> Finding {
    let finding = Finding::new(
        key,
        unavailable(&format!("`{}`", key.slice(context.text)), Effect::Line),
    );
    Finding {
        severity: settings_severity(spec),
        ..finding
    }
}

fn settings_severity(spec: &SpecSection) -> Option<Severity> {
    DLL_SETTINGS_SECTIONS
        .contains(&spec.name.as_str())
        .then_some(Severity::Hint)
}
