//! Read-only lookups over the merged spec (`xxmi query`). Matching follows `Lookup`, so a prefix
//! command like `commandlistFoo` finds `commandlist`.

use serde::Serialize;

use crate::lookup::Lookup;
use crate::spec::{CommandForm, Meta, Spec, SpecCommand, SpecSection};

/// Field order is the TypeScript object order of every entry kind, so one struct serves them all.
#[derive(Debug, Serialize)]
pub struct SpecEntry {
    pub kind: &'static str,
    pub name: String,
    /// For keys: the spec section that reads the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// For members: whether the DLL reads it on a resource or on a shader stage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
    pub xxmi: bool,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// One line of facts the DLL gives (value type, argument count, precedence, ...).
    pub facts: Vec<String>,
    /// Keyword values (commands) or enum values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct SpecListItem {
    pub name: String,
    /// First line of the doc, or empty when the entry has no overlay doc.
    pub summary: String,
    pub xxmi: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<&'static str>,
}

/// `[Name]`, `->Name` and surrounding space removed.
pub fn normalize(name: &str) -> &str {
    let trimmed = name.trim();
    let unwrapped = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    unwrapped.strip_prefix("->").unwrap_or(unwrapped)
}

fn entry(kind: &'static str, name: String, meta: &Meta, facts: Vec<String>) -> SpecEntry {
    let present = |text: &Option<String>| text.clone().filter(|t| !t.is_empty());
    SpecEntry {
        kind,
        name,
        section: None,
        target: None,
        xxmi: meta.xxmi,
        source: meta.source.clone(),
        doc: present(&meta.doc),
        example: present(&meta.example),
        since: present(&meta.since),
        deprecated: present(&meta.deprecated),
        note: present(&meta.note),
        facts,
        values: None,
        form: None,
    }
}

fn command_entry(command: &SpecCommand) -> SpecEntry {
    let mut facts = vec![format!("{} command", command.kind.as_str())];
    if command.form == Some(CommandForm::Value) {
        facts.push("written as the value, whatever the key is".to_string());
    }
    facts.push(
        match command.match_kind {
            crate::spec::MatchKind::Prefix => "matches by prefix",
            crate::spec::MatchKind::Exact => "exact name",
        }
        .to_string(),
    );
    if let Some(count) = command.arg_count {
        facts.push(format!("{count} comma-separated arguments"));
    }
    if command.indirect {
        facts.push("first argument is an indirect-arguments resource".to_string());
    }

    let mut found = entry("command", command.name.clone(), &command.meta, facts);
    if !command.values.is_empty() {
        found.values = Some(command.values.iter().map(|v| v.name.clone()).collect());
    }
    found.form = command.form.map(|_| "value");
    found
}

fn lookup_commands(spec: &Spec, lookup: &Lookup, name: &str) -> Vec<SpecEntry> {
    let lower = name.to_lowercase();
    let mut matches: Vec<&SpecCommand> = spec
        .commands
        .iter()
        .filter(|c| c.name.to_lowercase() == lower)
        .collect();
    if let Some(via_prefix) = lookup.command(name)
        && !matches.iter().any(|c| std::ptr::eq(*c, via_prefix))
    {
        matches.push(via_prefix);
    }
    matches.into_iter().map(command_entry).collect()
}

fn lookup_functions(spec: &Spec, name: &str) -> Vec<SpecEntry> {
    let lower = name.to_lowercase();
    spec.functions
        .iter()
        .filter(|f| f.name.to_lowercase() == lower)
        .map(|f| {
            let facts = vec!["built-in math function".to_string()];
            entry("function", f.name.clone(), &f.meta, facts)
        })
        .collect()
}

fn lookup_members(spec: &Spec, name: &str) -> Vec<SpecEntry> {
    let lower = name.to_lowercase();
    spec.resource_members
        .iter()
        .filter(|m| m.name.to_lowercase() == lower)
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

/// Commands, functions and `->members` named `name`, in that order (`xxmi query command`).
pub fn lookup_command_like(spec: &Spec, lookup: &Lookup, name: &str) -> Vec<SpecEntry> {
    let name = normalize(name);
    if name.is_empty() {
        return Vec::new();
    }

    let mut found = lookup_commands(spec, lookup, name);
    found.extend(lookup_functions(spec, name));
    found.extend(lookup_members(spec, name));
    found
}

pub fn lookup_operators(spec: &Spec, name: &str) -> Vec<SpecEntry> {
    let name = normalize(name);
    if name.is_empty() {
        return Vec::new();
    }

    spec.operators
        .tokens
        .iter()
        .filter(|t| t.token == name)
        .map(|t| {
            let level = spec
                .operators
                .precedence
                .iter()
                .find(|p| p.operators.contains(&t.token));
            let facts = level.map_or_else(Vec::new, |level| {
                vec![
                    format!("precedence level {} (0 binds tightest)", level.level),
                    format!("{}-associative", level.associativity.as_str()),
                    if level.unary { "unary" } else { "binary" }.to_string(),
                ]
            });
            entry("operator", t.token.clone(), &t.meta, facts)
        })
        .collect()
}

fn summarize(doc: Option<&str>) -> String {
    doc.and_then(|d| d.lines().next())
        .map_or_else(String::new, |line| line.trim().to_string())
}

fn item(name: String, meta: &Meta) -> SpecListItem {
    SpecListItem {
        name,
        summary: summarize(meta.doc.as_deref()),
        xxmi: meta.xxmi,
        target: None,
        form: None,
    }
}

/// Every section kind the DLL knows.
pub fn list_sections(spec: &Spec) -> Vec<SpecListItem> {
    spec.sections
        .iter()
        .map(|s| item(s.name.clone(), &s.meta))
        .collect()
}

/// The keys a section reads, or `None` when `section` names no known section.
pub fn list_keys(lookup: &Lookup, section: &str) -> Option<Vec<SpecListItem>> {
    let found: &SpecSection = lookup.section_kind(normalize(section))?;
    Some(
        found
            .keys
            .iter()
            .map(|k| item(k.name.clone(), &k.meta))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::bundled_lookup;

    #[test]
    fn normalize_strips_brackets_and_arrow() {
        assert_eq!(normalize(" [Resource] "), "Resource");
        assert_eq!(normalize("->Region"), "Region");
    }

    #[test]
    fn operator_entry_carries_precedence() {
        let lookup = bundled_lookup();
        let found = lookup_operators(lookup.spec(), "+");
        assert_eq!(found.len(), 1);
        assert!(found[0].facts[0].starts_with("precedence level"));
    }

    #[test]
    fn keys_of_an_unknown_section_are_none() {
        assert!(list_keys(bundled_lookup(), "NoSuchSection").is_none());
        assert!(list_keys(bundled_lookup(), "[Resource]").is_some());
    }
}
