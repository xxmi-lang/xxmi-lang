//! XM307, for what the parser finds inside lines: functions, operators, resource members,
//! declaration modifiers, run-time values and resource pools.

use std::collections::HashSet;

use super::xxmi_only::{Effect, unavailable};
use super::{Finding, RuleContext};
use crate::lookup::eq_caseless;
use crate::model::{Usage, UsageKind};

/// The enum table of the names `x = frame_number` and `if fps > 30` read.
const RUNTIME_VALUES: &str = "ParamOverrideTypeNames";

/// `ignored` holds the sections XM307 already reports whole.
pub fn check(context: &RuleContext, ignored: &HashSet<usize>) -> Vec<Finding> {
    let in_ignored = |section: Option<usize>| section.is_some_and(|s| ignored.contains(&s));

    let usages = context
        .file
        .usages
        .iter()
        .filter(|usage| !in_ignored(usage.section))
        .filter_map(|usage| {
            let what = unavailable_name(context, usage)?;
            let effect = effect_of(usage.kind);
            Some(Finding::new(usage.span, unavailable(&what, effect)))
        });
    let pools = context
        .file
        .references
        .iter()
        .filter(|reference| !in_ignored(reference.section))
        .filter_map(|reference| {
            let what = unavailable_pool(context, reference)?;
            Some(Finding::new(
                reference.span,
                unavailable(&what, Effect::Expression),
            ))
        });
    usages.chain(pools).collect()
}

fn effect_of(kind: UsageKind) -> Effect {
    match kind {
        UsageKind::Modifier | UsageKind::RunValue => Effect::Line,
        _ => Effect::Expression,
    }
}

/// How the message names the construct, if the spec entry for it is `gimi: false`.
fn unavailable_name(context: &RuleContext, usage: &Usage) -> Option<String> {
    let spec = context.lookup.spec();
    let text = usage.text.as_str();

    let missing = match usage.kind {
        UsageKind::Function => spec
            .functions
            .iter()
            .find(|f| eq_caseless(&f.name, text))
            .is_some_and(|f| !f.meta.gimi),
        UsageKind::Operator => spec
            .operators
            .tokens
            .iter()
            .find(|t| t.token == text)
            .is_some_and(|t| !t.meta.gimi),
        UsageKind::Member => is_missing_member(context, text),
        UsageKind::Modifier => context
            .lookup
            .command(text)
            .is_some_and(|command| !command.meta.gimi),
        UsageKind::RuntimeValue => context
            .lookup
            .enum_table(RUNTIME_VALUES)
            .and_then(|table| table.values.iter().find(|v| eq_caseless(&v.name, text)))
            .is_some_and(|value| !value.meta.gimi),
        UsageKind::RunValue => context
            .lookup
            .value_commands()
            .find(|command| eq_caseless(&command.name, text))
            .is_some_and(|command| !command.meta.gimi),
    };
    missing.then(|| describe(usage))
}

/// A member name that shader targets and resource targets both have is missing only if neither
/// entry exists in the GIMI DLL.
fn is_missing_member(context: &RuleContext, name: &str) -> bool {
    let mut members = context
        .lookup
        .spec()
        .resource_members
        .iter()
        .filter(|m| eq_caseless(&m.name, name))
        .peekable();
    members.peek().is_some() && members.all(|m| !m.meta.gimi)
}

fn describe(usage: &Usage) -> String {
    let text = &usage.text;
    match usage.kind {
        UsageKind::Function => format!("`{text}()`"),
        UsageKind::Operator => format!("Operator `{text}`"),
        UsageKind::Member => format!("`->{text}`"),
        UsageKind::Modifier | UsageKind::RuntimeValue | UsageKind::RunValue => format!("`{text}`"),
    }
}

/// A pool the file defines and uses, when the section kind that declares pools is `gimi: false`.
/// An unresolved pool is XM202's.
fn unavailable_pool(context: &RuleContext, reference: &crate::model::Reference) -> Option<String> {
    let declaring = context
        .lookup
        .spec()
        .sections
        .iter()
        .find(|s| context.lookup.symbol_kind(s) == Some(reference.kind))?;
    if declaring.meta.gimi {
        return None;
    }

    let file = context.file;
    context
        .symbols()
        .resolve_section(reference.kind, &reference.text, &file.namespace)?;
    Some(format!(
        "`{}`, a [{}] section,",
        reference.text, declaring.name
    ))
}
