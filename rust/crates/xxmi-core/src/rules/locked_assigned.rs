//! XM212: an assignment to a `locked` global.

use super::{Finding, RuleContext};
use crate::model::{Access, IniFile, Reference, SymbolKind};
use crate::spec::SectionKind;

const PRESET_SECTIONS: [&str; 2] = ["Key", "Preset"];

/// Command-list assignments, found by the parser.
fn command_list_writes(file: &IniFile) -> impl Iterator<Item = &Reference> {
    file.references
        .iter()
        .filter(|r| r.kind == SymbolKind::Variable && r.access == Access::Write)
        .filter(|r| {
            r.section
                .and_then(|section| file.sections[section].spec)
                .is_some_and(|spec| spec.kind == SectionKind::CommandList)
        })
}

/// The `$var = …` keys of [Key] and [Preset] sections: they aren't command lists,
/// so the parser records no reference for them.
fn preset_writes<'a>(
    context: &'a RuleContext,
    file: &'a IniFile,
) -> impl Iterator<Item = Reference> + 'a {
    file.sections
        .iter()
        .enumerate()
        .filter(|(_, section)| {
            section
                .spec
                .is_some_and(|spec| PRESET_SECTIONS.contains(&spec.name.as_str()))
        })
        .flat_map(move |(index, section)| {
            section.lines.iter().filter_map(move |line| {
                let key = line.scanned.key?;
                let text = key.slice(context.text);
                text.starts_with('$').then(|| Reference {
                    kind: SymbolKind::Variable,
                    text: text.to_string(),
                    span: key,
                    section: Some(index),
                    access: Access::Write,
                })
            })
        })
}

fn locked_finding(context: &RuleContext, reference: &Reference) -> Option<Finding> {
    context
        .symbols()
        .resolve_variable_symbol(reference, context.file_id, context.file)
        .is_some_and(|symbol| symbol.locked)
        .then(|| {
            Finding::new(
                reference.span,
                format!(
                    "Global {} is `locked`; assigning it is an error and the DLL drops the line.",
                    reference.text
                ),
            )
        })
}

/// ParseCommandListVariableAssignment (CommandList.cpp) reads the target through GetVariable,
/// which fails with "Unable to assign value to <locked> variable" in any namespace,
/// and the line is dropped. Override::ParseIniSection (Override.cpp) reads the `$var` keys of
/// [Key] and [Preset] sections through the same GetVariable and skips the line the same way.
pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    let listed = command_list_writes(file).filter_map(|r| locked_finding(context, r));
    let presets = preset_writes(context, file).filter_map(|r| locked_finding(context, &r));

    listed.chain(presets).collect()
}
