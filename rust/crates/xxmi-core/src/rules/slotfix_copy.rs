//! XM305: a `Resource\ZZMI\<Name>` slot filled by a copy instead of a reference (zzmi profile).
//!
//! With no `ref`/`copy` the DLL copies when the destination is a custom resource
//! (`parse_resource_copy_operation`, CommandList.cpp), and `ResourceCopyOperation::run`
//! then copies on every execution of the line, which is every draw its command list runs for.

use super::copy_value::{names_copy_type, resource_source};
use super::{Finding, FixSpec, RuleContext};
use crate::model::{LineModel, SymbolKind};
use crate::scan::Span;
use crate::spec::SectionKind;

/// The namespace of ZZMI's SlotFix slots; `ZZMIv1` is the text printer's, not these.
const SLOT_PREFIX: &str = "resource\\zzmi\\";

pub fn check(context: &RuleContext) -> Vec<Finding> {
    if !context.is_zzmi() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for section in &context.file.sections {
        if section.spec.map(|spec| spec.kind) != Some(SectionKind::CommandList) {
            continue;
        }
        findings.extend(
            section
                .lines
                .iter()
                .filter_map(|line| check_line(context, line)),
        );
    }
    findings
}

fn check_line(context: &RuleContext, line: &LineModel) -> Option<Finding> {
    let (key, value) = (line.scanned.key?, line.scanned.value?);
    let command = line.command.as_deref()?;
    if !is_known_slot(context, command) {
        return None;
    }

    let source = value.slice(context.text);
    if names_copy_type(source) || resource_source(source).is_none() {
        return None;
    }

    Some(Finding {
        span: key,
        message: format!(
            "`{}` is copied on every draw: with no `ref` the DLL copies into a custom resource.",
            key.slice(context.text)
        ),
        fix: Some(insert_ref(value)),
        severity: None,
    })
}

/// A slot the loaded library really declares, so a typo (XM202's) isn't reported twice.
fn is_known_slot(context: &RuleContext, command: &str) -> bool {
    let name = command.strip_prefix(SLOT_PREFIX);
    name.is_some_and(|name| !name.is_empty() && !name.contains('\\'))
        && context
            .symbols()
            .resolve_section(SymbolKind::Resource, command, &context.file.namespace)
            .is_some()
}

fn insert_ref(value: Span) -> FixSpec {
    FixSpec {
        title: "Insert `ref`".to_string(),
        span: Span {
            start: value.start,
            end: value.start,
        },
        new_text: "ref ".to_string(),
        also: Vec::new(),
    }
}
