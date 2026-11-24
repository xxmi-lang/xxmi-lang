//! XM306: `override_byte_stride` differs from the `stride` of the resource the same
//! TextureOverride binds to `vb0` (zzmi profile).
//!
//! The DLL never compares them: `IniHandler.cpp` only multiplies the value
//! by `override_vertex_count` into the buffer size,
//! and `HackerDevice.cpp` grows the matching buffer to it.
//! The hash may match any vertex buffer, so a difference can be intended;
//! this is a hint, and it reports only what the files settle:
//! one unconditional `vb0` line and no other vertex buffer bound (any other is skipped,
//! unless the value is the sum of all their strides), no `handling = skip`,
//! a resource with its own data and a single literal `stride` that nothing copies into,
//! and no called command list or custom shader that might bind a vertex buffer.

use std::collections::{BTreeSet, HashSet};

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

    let slots = vertex_slots(section);
    let message = if slots.len() > 1 {
        summed_message(context, section, &slots, declared, &vb0)?
    } else if skips_draw(context.text, section) {
        return None;
    } else {
        mismatch_message(declared, &vb0)
    };

    Some(Finding::new(value, message))
}

/// Reports a value equal to the strides of every bound buffer added up;
/// any other value with several buffers bound is left alone.
fn summed_message(
    context: &RuleContext,
    section: &SectionModel,
    slots: &BTreeSet<u32>,
    declared: u32,
    vb0: &Bound,
) -> Option<String> {
    let strides: Vec<u32> = slots
        .iter()
        .map(|&slot| bound(context, section, slot).map(|b| b.stride))
        .collect::<Option<_>>()?;
    if strides.iter().sum::<u32>() != declared {
        return None;
    }

    let names: Vec<String> = slots.iter().map(|slot| format!("vb{slot}")).collect();
    let (last, rest) = names.split_last()?;
    let parts: Vec<String> = strides.iter().map(u32::to_string).collect();
    Some(format!(
        "`override_byte_stride = {declared}` looks like the strides of {} and {last} added up ({}); `vb0` alone is {}.",
        rest.join(", "),
        parts.join(" + "),
        vb0.stride
    ))
}

fn mismatch_message(declared: u32, vb0: &Bound) -> String {
    format!(
        "`override_byte_stride = {declared}` is not `stride = {}` of `vb0 = {}`; the DLL doesn't compare them and the hash may match another buffer, so check it is intended.",
        vb0.stride, vb0.resource
    )
}

/// The slots of every `vb<n>` line, in any branch.
fn vertex_slots(section: &SectionModel) -> BTreeSet<u32> {
    section.lines.iter().filter_map(vertex_slot).collect()
}

fn vertex_slot(line: &LineModel) -> Option<u32> {
    line.command.as_deref()?.strip_prefix("vb")?.parse().ok()
}

/// A `handling = skip` override replaces the draw, so a larger stride is deliberate.
fn skips_draw(text: &str, section: &SectionModel) -> bool {
    section.lines.iter().any(|line| {
        line.command.as_deref() == Some("handling")
            && line
                .scanned
                .value
                .is_some_and(|value| value.slice(text).eq_ignore_ascii_case("skip"))
    })
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

/// Whether a command list or custom shader the section runs, at any depth,
/// might bind a vertex buffer
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
                binds_vertex_buffer(context, file, target)
                    || calls_may_bind(context, file, target, visited)
            }
            Some(Origin::File { .. }) => true,
        };
        if may_bind {
            return true;
        }
    }
    false
}

fn binds_vertex_buffer(context: &RuleContext, file: usize, section: usize) -> bool {
    context.workspace.files()[file].sections[section]
        .lines
        .iter()
        .any(|line| vertex_slot(line).is_some())
}
