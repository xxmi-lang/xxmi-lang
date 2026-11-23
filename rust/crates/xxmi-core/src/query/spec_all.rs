//! Lookups across every spec kind (`lookup_spec`, `list_spec`):
//! the MCP `spec_lookup` and `spec_list` tools.
//! Port of `lookupSpec` and `listSpec` in packages/core/src/query/spec.ts.

use crate::lookup::Lookup;
use crate::spec::{MemberTarget, SectionKind, SpecSection};

use super::spec::{
    SpecEntry, SpecListItem, entry, item, lookup_commands, lookup_functions, lookup_operators,
    normalize,
};

pub const SPEC_ENTRY_KINDS: [&str; 7] = [
    "section", "key", "command", "operator", "function", "member", "enum",
];

#[derive(Default)]
pub struct SpecQuery<'a> {
    pub name: &'a str,
    pub kind: Option<&'a str>,
    /// Restricts `key` lookups to the section named (or whose prefix matches) this.
    pub section: Option<&'a str>,
    /// Restricts `member` lookups to resource or shader members.
    pub target: Option<MemberTarget>,
}

fn section_entry(section: &SpecSection) -> SpecEntry {
    let facts = vec![
        if section.prefix {
            "prefix match (any section name starting with it)"
        } else {
            "exact name"
        }
        .to_string(),
        if section.kind == SectionKind::CommandList {
            "command-list section"
        } else {
            "regular section"
        }
        .to_string(),
        format!("{} keys", section.keys.len()),
    ];
    entry("section", section.name.clone(), &section.meta, facts)
}

fn lookup_sections(lookup: &'static Lookup, name: &str) -> Vec<SpecEntry> {
    let lower = name.to_lowercase();
    let exact = lookup
        .spec()
        .sections
        .iter()
        .find(|s| s.name.to_lowercase() == lower);
    exact
        .or_else(|| lookup.section_kind(name))
        .map(section_entry)
        .into_iter()
        .collect()
}

fn lookup_keys(lookup: &'static Lookup, query: &SpecQuery, name: &str) -> Vec<SpecEntry> {
    let only = query
        .section
        .filter(|s| !s.is_empty())
        .map(|s| lookup.section_kind(normalize(s)));
    if only.is_some_and(|found| found.is_none()) {
        return Vec::new();
    }

    let sections: Vec<&'static SpecSection> = match only.flatten() {
        Some(section) => vec![section],
        None => lookup.spec().sections.iter().collect(),
    };
    sections
        .into_iter()
        .filter_map(|section| {
            let key = lookup.key(section, name)?;
            let mut facts = vec![format!("type {}", key.value_type)];
            if key.repeatable {
                facts.push("repeatable".to_string());
            }
            let ellipsis = if section.prefix { "…" } else { "" };
            facts.push(format!("read in [{}{ellipsis}]", section.name));

            let mut found = entry("key", key.name.clone(), &key.meta, facts);
            found.section = Some(section.name.clone());
            Some(found)
        })
        .collect()
}

fn lookup_members(lookup: &Lookup, name: &str, target: Option<MemberTarget>) -> Vec<SpecEntry> {
    let lower = name.to_lowercase();
    lookup
        .spec()
        .resource_members
        .iter()
        .filter(|m| m.name.to_lowercase() == lower && target.is_none_or(|t| m.target == t))
        .map(|m| {
            let args = if m.args.is_empty() {
                "none".to_string()
            } else {
                m.args.join(", ")
            };
            let facts = vec![
                format!("{} member", m.target.as_str()),
                format!("arguments: {args}"),
            ];
            let mut found = entry("member", format!("->{}", m.name), &m.meta, facts);
            found.target = Some(m.target.as_str());
            found
        })
        .collect()
}

fn lookup_enums(lookup: &Lookup, name: &str) -> Vec<SpecEntry> {
    let lower = name.to_lowercase();
    lookup
        .spec()
        .enums
        .iter()
        .filter(|e| e.name.to_lowercase() == lower)
        .map(|e| {
            let mut found = entry(
                "enum",
                e.name.clone(),
                &e.meta,
                vec!["enum table".to_string()],
            );
            found.values = Some(e.values.iter().map(|v| v.name.clone()).collect());
            found
        })
        .collect()
}

/// All spec entries named `query.name`, of `query.kind` or of any kind.
pub fn lookup_spec(lookup: &'static Lookup, query: &SpecQuery) -> Vec<SpecEntry> {
    let name = normalize(query.name);
    if name.is_empty() {
        return Vec::new();
    }

    let spec = lookup.spec();
    let kinds: Vec<&str> = query
        .kind
        .map_or_else(|| SPEC_ENTRY_KINDS.to_vec(), |k| vec![k]);
    kinds
        .into_iter()
        .flat_map(|kind| match kind {
            "section" => lookup_sections(lookup, name),
            "key" => lookup_keys(lookup, query, name),
            "command" => lookup_commands(spec, lookup, name),
            "operator" => lookup_operators(spec, name),
            "function" => lookup_functions(spec, name),
            "member" => lookup_members(lookup, name, query.target),
            _ => lookup_enums(lookup, name),
        })
        .collect()
}

/// Names (with one-line docs) of one kind. `key` needs a known `section`;
/// otherwise the message is the error the tool shows.
pub fn list_spec(
    lookup: &'static Lookup,
    kind: &str,
    section: Option<&str>,
) -> Result<Vec<SpecListItem>, String> {
    let spec = lookup.spec();
    Ok(match kind {
        "section" => spec
            .sections
            .iter()
            .map(|s| item(s.name.clone(), &s.meta))
            .collect(),
        "key" => {
            let found = section
                .filter(|s| !s.is_empty())
                .and_then(|s| lookup.section_kind(normalize(s)))
                .ok_or("listing keys needs a known section, e.g. \"Resource\"")?;
            found
                .keys
                .iter()
                .map(|k| item(k.name.clone(), &k.meta))
                .collect()
        }
        "command" => spec
            .commands
            .iter()
            .map(|c| {
                let mut listed = item(c.name.clone(), &c.meta);
                listed.form = c.form.map(|_| "value");
                listed
            })
            .collect(),
        "operator" => spec
            .operators
            .tokens
            .iter()
            .map(|t| item(t.token.clone(), &t.meta))
            .collect(),
        "function" => spec
            .functions
            .iter()
            .map(|f| item(f.name.clone(), &f.meta))
            .collect(),
        "member" => spec
            .resource_members
            .iter()
            .map(|m| {
                let mut listed = item(format!("->{}", m.name), &m.meta);
                listed.target = Some(m.target.as_str());
                listed
            })
            .collect(),
        _ => spec
            .enums
            .iter()
            .map(|e| item(e.name.clone(), &e.meta))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::bundled_lookup;

    #[test]
    fn finds_entries_of_every_kind() {
        let lookup = bundled_lookup();
        let query = |name| SpecQuery {
            name,
            ..SpecQuery::default()
        };
        assert_eq!(lookup_spec(lookup, &query("Resource"))[0].kind, "section");
        assert!(lookup_spec(lookup, &query("nosuchthing")).is_empty());

        let keyed = SpecQuery {
            name: "filename",
            kind: Some("key"),
            section: Some("[Resource]"),
            ..SpecQuery::default()
        };
        assert_eq!(
            lookup_spec(lookup, &keyed)[0].section.as_deref(),
            Some("Resource")
        );
    }

    #[test]
    fn listing_keys_needs_a_known_section() {
        let lookup = bundled_lookup();
        assert!(list_spec(lookup, "key", None).is_err());
        assert!(
            !list_spec(lookup, "key", Some("Resource"))
                .unwrap()
                .is_empty()
        );
        assert!(!list_spec(lookup, "member", None).unwrap().is_empty());
    }
}
