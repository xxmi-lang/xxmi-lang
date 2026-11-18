//! XM102: a key the section's parser doesn't read (port of XM102 in
//! packages/core/src/lint/rules/structure.ts).

use super::{Finding, RuleContext, replace_fix};
use crate::model::{LineModel, SectionModel};
use crate::scan::Span;
use crate::spec::{DynamicKeyKind, SectionKind, SpecSection};
use crate::suggest::did_you_mean;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section.spec else {
            continue;
        };
        if is_free_form(spec) {
            continue;
        }

        let known: Vec<&str> = spec.keys.iter().map(|k| k.name.as_str()).collect();
        for line in &section.lines {
            if let Some(finding) = check_line(context, section, spec, &known, line) {
                findings.push(finding);
            }
        }
    }
    findings
}

/// Sections whose content the DLL doesn't read as keys (e.g. `[Profile]`).
fn is_free_form(spec: &SpecSection) -> bool {
    spec.allows_bare_lines && spec.keys.is_empty() && spec.kind == SectionKind::Regular
}

fn check_line(
    context: &RuleContext,
    section: &SectionModel,
    spec: &'static SpecSection,
    known: &[&str],
    line: &LineModel,
) -> Option<Finding> {
    let key_span = line.scanned.key?;
    let key = key_span.slice(context.text);

    if spec.kind == SectionKind::CommandList {
        return check_command_list_line(context, spec, line, key, known);
    }
    check_regular_line(context, section, spec, key_span, known)
}

fn check_command_list_line(
    context: &RuleContext,
    spec: &'static SpecSection,
    line: &LineModel,
    key: &str,
    known: &[&str],
) -> Option<Finding> {
    // The grammar parses anything it doesn't recognise as a command, resource copy or assignment
    // as a generic `key = value` setting.
    if line.statement.as_deref() != Some("setting_statement") {
        return None;
    }

    let lowered = key.to_lowercase();
    let command = line.command.as_deref().unwrap_or(&lowered);
    if context.lookup.key(spec, command).is_some() || context.lookup.command(command).is_some() {
        return None;
    }

    let mut candidates = known.to_vec();
    for name in context.lookup.key_command_names() {
        candidates.push(name);
    }
    let content = line.scanned.content;
    Some(Finding {
        span: content,
        message: format!(
            "Unrecognised entry: {}.{}",
            content.slice(context.text),
            did_you_mean(command, candidates.iter().copied())
        ),
        fix: None,
    })
}

fn check_regular_line(
    context: &RuleContext,
    section: &SectionModel,
    spec: &'static SpecSection,
    key_span: Span,
    known: &[&str],
) -> Option<Finding> {
    let key = key_span.slice(context.text);
    if context.lookup.key(spec, key).is_some() || is_dynamic_key(spec, key) {
        return None;
    }

    let lower = key.to_lowercase();
    if spec
        .removed_keys
        .iter()
        .any(|k| k.name.to_lowercase() == lower)
    {
        return Some(Finding {
            span: key_span,
            message: format!(
                "`{key}` is read by vanilla 3DMigoto but not by XXMI; the DLL ignores it."
            ),
            fix: None,
        });
    }

    let hint = if known.is_empty() {
        " XXMI reads no keys from this section.".to_string()
    } else {
        did_you_mean(key, known.iter().copied())
    };
    let name = section.name.slice(context.text);
    Some(Finding {
        span: key_span,
        message: format!("Unknown key `{key}` in [{name}]; the DLL ignores it.{hint}"),
        fix: replace_fix(key_span, key, known, ""),
    })
}

/// `$name` variables and ini params (`x`, `y1`, `w12`) the section accepts as keys.
fn is_dynamic_key(spec: &SpecSection, key: &str) -> bool {
    spec.dynamic_keys.iter().any(|dynamic| match dynamic.kind {
        DynamicKeyKind::Variable => key.starts_with('$'),
        DynamicKeyKind::IniParam => is_ini_param(key),
    })
}

/// `^[xyzw]\d{0,3}$`, case-insensitively (the grammar's `ini_parameter`).
fn is_ini_param(key: &str) -> bool {
    let mut chars = key.chars();
    let letter = chars
        .next()
        .is_some_and(|c| matches!(c.to_ascii_lowercase(), 'x' | 'y' | 'z' | 'w'));
    let digits = chars.as_str();
    letter && digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_params() {
        for key in ["x", "Y1", "w123", "z"] {
            assert!(is_ini_param(key), "{key}");
        }
        for key in ["x1234", "xa", "", "v1", "x-1"] {
            assert!(!is_ini_param(key), "{key}");
        }
    }
}
