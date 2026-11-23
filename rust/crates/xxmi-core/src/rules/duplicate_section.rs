//! XM107: a section name the DLL has already seen in an earlier file (it keeps the first).

use std::collections::HashMap;

use super::duplicate_grade::grade;
use super::loaded_files::{loaded_files, loads_before};
use super::{Finding, RuleContext};
use crate::model::{IniFile, SectionModel};
use crate::paths::relative_path;

type Copy<'a> = (usize, &'a SectionModel);

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let winners = winners(context);
    let file = context.file;

    let mut findings = Vec::new();
    for section in &file.sections {
        if !clashes(context, file, section) {
            continue;
        }
        let key = section.qualified_name.to_lowercase();
        let Some(&(first_id, first)) = winners.get(&key) else {
            continue;
        };
        if !std::ptr::eq(first, section) {
            let finding =
                duplicate_finding(context, (context.file_id, file), section, (first_id, first));
            findings.push(finding);
        }
    }
    findings
}

/// The copy the DLL keeps for each name: the first in its own load order (see `load_order`).
fn winners<'a>(context: &RuleContext<'a>) -> HashMap<String, Copy<'a>> {
    let mut winners: HashMap<String, Copy> = HashMap::new();
    for (id, other) in loaded_files(context) {
        for section in &other.sections {
            if !clashes(context, other, section) {
                continue;
            }
            let key = section.qualified_name.to_lowercase();
            let beats =
                |&(first_id, _): &Copy| first_id != id && loads_before(context, id, first_id);
            if winners.get(&key).is_none_or(beats) {
                winners.insert(key, (id, section));
            }
        }
    }
    winners
}

fn clashes(context: &RuleContext, file: &IniFile, section: &SectionModel) -> bool {
    let prefixed = context
        .lookup
        .section_prefix(file.slice(section.name))
        .is_some();
    // Global sections from included files merge into the main one instead of duplicating.
    file.namespace.is_empty() || prefixed
}

fn duplicate_finding(
    context: &RuleContext,
    (id, file): (usize, &IniFile),
    section: &SectionModel,
    (first_id, first): (usize, &SectionModel),
) -> Finding {
    let name = file.slice(section.name);
    let first_line = first.header.line + 1;
    if first_id == id {
        return Finding::new(
            section.name,
            message(name, &format!("line {first_line}"), None),
        );
    }

    let first_file = &context.workspace.files()[first_id];
    let location = format!("{}:{first_line}", relative_location(file, first_file));
    let graded = grade(context, (file, section), (first_file, first));
    Finding {
        severity: Some(graded.severity()),
        ..Finding::new(section.name, message(name, &location, graded.note()))
    }
}

/// Relative to the duplicate's folder, so the message doesn't depend on where the package lives.
pub(super) fn relative_location(from: &IniFile, to: &IniFile) -> String {
    let folder = from.path.parent().unwrap_or(&from.path);
    relative_path(folder, &to.path).replace(std::path::MAIN_SEPARATOR, "/")
}

fn message(name: &str, location: &str, note: Option<&str>) -> String {
    let note = note.map(|n| format!("; {n}")).unwrap_or_default();
    format!("Duplicate section found: [{name}] (first at {location}; the DLL uses the first{note})")
}
