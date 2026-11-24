//! XM306: `override_byte_stride` differs from the `stride` of the resource the same
//! TextureOverride binds to `vb0` (zzmi profile).
//!
//! The DLL never compares them: `IniHandler.cpp` only multiplies the value by
//! `override_vertex_count` into the buffer size (`HackerDevice.cpp` grows the matching buffer to it).
//! So this is a convention check, and it reports only what the files settle:
//! one unconditional `vb0` line, a resource with its own data and a single literal `stride`
//! that nothing copies into, and no called command list that might rebind `vb0`.

use std::collections::HashSet;

use super::copy_value::resource_source;
use super::keywords::{FlowKeyword, flow_keyword};
use super::{Finding, RuleContext};
use crate::model::{LineModel, SectionModel, SymbolKind};
use crate::scan::Span;
use crate::spec::SectionKind;
use crate::workspace::symbols::Origin;

/// A vertex buffer slot resolved to the stride its resource declares.
struct Bound<'a> {
    resource: &'a str,
    stride: u32,
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    if !context.is_zzmi() {
        return Vec::new();
    }

    let sections = context.file.sections.iter().enumerate();
    sections
        .filter(|(_, section)| {
            section
                .spec
                .is_some_and(|spec| spec.name == "TextureOverride")
        })
        .filter_map(|(index, section)| check_section(context, index, section))
        .collect()
}

fn check_section(context: &RuleContext, index: usize, section: &SectionModel) -> Option<Finding> {
    let value = single_value(context.text, &section.lines, "override_byte_stride")?;
    let declared: u32 = value.slice(context.text).parse().ok()?;
    // Without a vertex count the DLL ignores the stride, so nothing is resized.
    single_value(context.text, &section.lines, "override_vertex_count")?;

    if calls_may_bind(context, context.file_id, index, &mut HashSet::new()) {
        return None;
    }
    let vb0 = bound(context, section, 0)?;
    if vb0.stride == declared {
        return None;
    }

    Some(Finding::new(
        value,
        message(context, section, declared, &vb0),
    ))
}

fn message(context: &RuleContext, section: &SectionModel, declared: u32, vb0: &Bound) -> String {
    let strides: Option<Vec<u32>> = (0..=2)
        .map(|slot| bound(context, section, slot).map(|b| b.stride))
        .collect();

    match strides {
        Some(strides) if strides.iter().sum::<u32>() == declared => {
            let parts: Vec<String> = strides.iter().map(u32::to_string).collect();
            format!(
                "`override_byte_stride = {declared}` looks like the strides of vb0, vb1 and vb2 added up ({}); `vb0` alone is {}.",
                parts.join(" + "),
                vb0.stride
            )
        }
        _ => format!(
            "`override_byte_stride = {declared}` differs from `stride = {}` of `vb0 = {}`.",
            vb0.stride, vb0.resource
        ),
    }
}

/// The value of a key that appears exactly once with a value.
fn single_value(text: &str, lines: &[LineModel], key: &str) -> Option<Span> {
    let mut found = lines
        .iter()
        .filter(|line| line.command.as_deref() == Some(key));
    let value = found.next()?.scanned.value?;
    (found.next().is_none() && !value.slice(text).is_empty()).then_some(value)
}

/// The stride of the resource `vb<slot>` binds, when the section binds it once and unconditionally.
fn bound<'a>(context: &'a RuleContext, section: &SectionModel, slot: u32) -> Option<Bound<'a>> {
    let command = format!("vb{slot}");
    let mut depth = 0usize;
    let mut binding = None;
    for line in &section.lines {
        match flow_keyword(line, context.text) {
            Some(FlowKeyword::If) => depth += 1,
            Some(FlowKeyword::Endif) => depth = depth.saturating_sub(1),
            _ => {}
        }
        if line.command.as_deref() != Some(command.as_str()) {
            continue;
        }
        if depth > 0 || binding.is_some() {
            return None;
        }
        binding = Some(line.scanned.value?.slice(context.text));
    }

    let resource = resource_source(binding?)?;
    let stride = resource_stride(context, resource)?;
    Some(Bound { resource, stride })
}

fn resource_stride(context: &RuleContext, name: &str) -> Option<u32> {
    let symbols = context.symbols();
    let symbol =
        symbols.resolve_section_symbol(SymbolKind::Resource, name, &context.file.namespace)?;
    if symbols.sections_named(&symbol.qualified_name).len() != 1 {
        return None;
    }

    let Origin::File {
        file,
        section: Some(section),
    } = symbol.origin
    else {
        return None;
    };
    let owner = &context.workspace.files()[file];
    let lines = &owner.sections[section].lines;
    // A resource filled by a copy takes the source's stride, not the declared one.
    let has_data = lines
        .iter()
        .any(|line| matches!(line.command.as_deref(), Some("filename" | "data")));
    if !has_data || is_copied_into(context, &symbol.qualified_name) {
        return None;
    }

    single_value(&owner.text, lines, "stride")?
        .slice(&owner.text)
        .parse()
        .ok()
}

fn is_copied_into(context: &RuleContext, qualified_name: &str) -> bool {
    let symbols = context.symbols();
    context.workspace.files().iter().any(|file| {
        file.sections
            .iter()
            .filter(|section| {
                section
                    .spec
                    .is_some_and(|s| s.kind == SectionKind::CommandList)
            })
            .flat_map(|section| &section.lines)
            .filter(|line| line.scanned.value.is_some())
            .filter_map(|line| line.command.as_deref())
            .filter_map(|command| {
                symbols.resolve_section(SymbolKind::Resource, command, &file.namespace)
            })
            .any(|resolved| resolved.eq_ignore_ascii_case(qualified_name))
    })
}

/// Whether a command list or custom shader the section runs might bind `vb0`
/// (or can't be followed: an unresolved target is not a certainty either way).
fn calls_may_bind(
    context: &RuleContext,
    file_id: usize,
    section: usize,
    visited: &mut HashSet<(usize, usize)>,
) -> bool {
    if !visited.insert((file_id, section)) {
        return false;
    }

    let file = &context.workspace.files()[file_id];
    let calls = file.references.iter().filter(|reference| {
        reference.section == Some(section)
            && matches!(
                reference.kind,
                SymbolKind::CommandList | SymbolKind::CustomShader
            )
    });
    for call in calls {
        let target =
            context
                .symbols()
                .resolve_section_symbol(call.kind, &call.text, &file.namespace);
        let may_bind = match target.map(|symbol| symbol.origin) {
            None => true,
            // Library snapshots hold no lines; the libraries bind no mod's meshes.
            Some(Origin::Snapshot { .. }) => false,
            Some(Origin::File {
                file,
                section: Some(target),
            }) => {
                binds_vb0(context, file, target) || calls_may_bind(context, file, target, visited)
            }
            Some(Origin::File { .. }) => true,
        };
        if may_bind {
            return true;
        }
    }
    false
}

fn binds_vb0(context: &RuleContext, file: usize, section: usize) -> bool {
    context.workspace.files()[file].sections[section]
        .lines
        .iter()
        .any(|line| line.command.as_deref() == Some("vb0"))
}
