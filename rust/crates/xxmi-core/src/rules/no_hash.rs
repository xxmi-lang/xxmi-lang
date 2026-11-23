//! XM112: a `[TextureOverride…]` the DLL has nothing to match with.

use super::{Finding, RuleContext};

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
        if section
            .spec
            .is_none_or(|spec| spec.name != "TextureOverride")
        {
            continue;
        }

        let matches_something = section.lines.iter().any(|line| {
            let key = line
                .scanned
                .key
                .map(|k| k.slice(context.text).to_lowercase());
            key.is_some_and(|k| k == "hash" || RESOURCE_MATCH_KEYS.contains(&k.as_str()))
        });
        if matches_something {
            continue;
        }

        // ParseTextureOverrideSections: "Section missing Hash= or valid match options".
        findings.push(Finding {
            span: section.name,
            message: format!(
                "[{}] has no `hash` and no resource `match_*` key (`match_type`, `match_format`, `match_width`, …), \
                 so the DLL skips it.",
                section.name.slice(context.text)
            ),
            fix: None,
            severity: None,
        });
    }
    findings
}
