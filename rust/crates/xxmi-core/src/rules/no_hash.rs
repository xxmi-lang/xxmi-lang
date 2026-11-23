//! XM112: a `[TextureOverride…]` or `[ShaderOverride…]` the DLL skips for lack of a `hash`, or whose
//! `match_*` keys it ignores.

use super::{Finding, RuleContext};
use crate::model::SectionModel;
use crate::scan::Span;

/// TEXTURE_OVERRIDE_FUZZY_MATCHES in IniHandler.cpp: the keys that stand in for `hash`.
/// `filter_index`, `match_priority` and the draw-call `match_first_index`-style keys don't count.
const RESOURCE_MATCH_KEYS: [&str; 15] = [
    "match_type",
    "match_usage",
    "match_bind_flags",
    "match_cpu_access_flags",
    "match_misc_flags",
    "match_byte_width",
    "match_stride",
    "match_mips",
    "match_format",
    "match_width",
    "match_height",
    "match_depth",
    "match_array",
    "match_msaa",
    "match_msaa_quality",
];

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        match section.spec.map(|spec| spec.name.as_str()) {
            Some("TextureOverride") => findings.extend(check_texture_override(context, section)),
            Some("ShaderOverride") => findings.extend(check_shader_override(context, section)),
            _ => {}
        }
    }
    findings
}

/// The section's keys, lower-cased, with their spans.
/// A key with an empty value is dropped at load ("No value found"), so it isn't a key.
fn keys<'a>(
    context: &'a RuleContext,
    section: &'a SectionModel,
) -> impl Iterator<Item = (String, Span)> + 'a {
    section.lines.iter().filter_map(|line| {
        let key = line.scanned.key?;
        if line
            .scanned
            .value
            .is_some_and(|value| value.start == value.end)
        {
            return None;
        }
        Some((key.slice(context.text).to_lowercase(), key))
    })
}

fn check_texture_override(context: &RuleContext, section: &SectionModel) -> Vec<Finding> {
    let name = section.name.slice(context.text);
    let has_hash = keys(context, section).any(|(key, _)| key == "hash");
    let matches: Vec<_> = keys(context, section)
        .filter(|(key, _)| RESOURCE_MATCH_KEYS.contains(&key.as_str()))
        .collect();

    match (has_hash, matches.is_empty()) {
        // ParseTextureOverrideSections: "Section missing Hash= or valid match options".
        (false, true) => vec![Finding::new(
            section.name,
            format!(
                "[{name}] has no `hash` and no resource `match_*` key (`match_type`, `match_format`, `match_width`, …), \
                 so the DLL skips it."
            ),
        )],
        // ParseTextureOverrideSections: "Cannot use hash= and match options together!".
        (true, false) => matches
            .into_iter()
            .map(|(key, span)| {
                Finding::new(
                    span,
                    format!("`{key}` is ignored: with `hash`, the DLL matches by hash only."),
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn check_shader_override(context: &RuleContext, section: &SectionModel) -> Option<Finding> {
    if keys(context, section).any(|(key, _)| key == "hash") {
        return None;
    }

    // ParseShaderOverrideSections: "Section missing Hash=", with no fuzzy fallback.
    Some(Finding::new(
        section.name,
        format!(
            "[{}] has no `hash`, so the DLL skips it (a ShaderOverride can't match any other way).",
            section.name.slice(context.text)
        ),
    ))
}
