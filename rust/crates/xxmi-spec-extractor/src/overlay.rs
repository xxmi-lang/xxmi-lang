//! Merges spec/overlay/*.toml documentation into an extracted spec.
//! Overlay tables address generated entries by name, case-insensitively;
//! an entry the DLL lacks is an error unless it is marked manual.

use std::fs;
use std::path::Path;

use toml::{Table, Value};

use crate::model::{
    Docs, Spec, SpecCommand, SpecEnum, SpecFunction, SpecKey, SpecOperatorToken,
    SpecResourceMember, SpecSection,
};

const DOC_FIELDS: [&str; 5] = ["doc", "example", "since", "deprecated", "note"];

/// One parsed `overlay/*.toml` file.
pub struct OverlayFile {
    pub file: String,
    pub overlay: Table,
}

/// Parses every `overlay/*.toml` under `root` in file-name order; unparsable files are problems.
pub fn read_overlays(root: &Path) -> (Vec<OverlayFile>, Vec<String>) {
    let mut overlays = Vec::new();
    let mut problems = Vec::new();
    let dir = root.join("overlay");
    let Ok(entries) = fs::read_dir(&dir) else {
        return (overlays, problems);
    };
    let mut files: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".toml"))
        .collect();
    files.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));

    for file in files {
        let text = fs::read_to_string(dir.join(&file)).map_err(|e| e.to_string());
        match text.and_then(|text| parse_overlay(&file, &text)) {
            Ok(overlay) => overlays.push(overlay),
            Err(message) => problems.push(format!("{file}: {message}")),
        }
    }
    (overlays, problems)
}

/// Parses the text of one overlay file; for callers that embed the overlays.
pub fn parse_overlay(file: &str, text: &str) -> Result<OverlayFile, String> {
    let overlay = text.parse::<Table>().map_err(|e| e.to_string())?;
    Ok(OverlayFile {
        file: file.to_string(),
        overlay,
    })
}

/// Merges `overlays` into `spec` in the given order; returns the problems found.
pub fn apply_overlays(spec: &mut Spec, overlays: &[OverlayFile]) -> Vec<String> {
    let mut problems = Problems::default();
    for overlay in overlays {
        problems.file = overlay.file.clone();
        apply_overlay(spec, &overlay.overlay, &mut problems);
    }
    problems.list
}

#[derive(Default)]
struct Problems {
    file: String,
    list: Vec<String>,
}

impl Problems {
    fn add(&mut self, path: &str, message: &str) {
        self.list.push(format!("{}: {path}: {message}", self.file));
    }
}

/// An entry an overlay can document.
trait Documented {
    fn entry_name(&self) -> &str;
    fn docs_mut(&mut self) -> &mut Docs;
    /// Sets a non-documentation field overlays may change (`valueType`, `args`).
    fn set_extra(&mut self, _field: &str, _value: &Value) {}
}

macro_rules! documented {
    ($type:ty, $name:ident) => {
        impl Documented for $type {
            fn entry_name(&self) -> &str {
                &self.$name
            }
            fn docs_mut(&mut self) -> &mut Docs {
                &mut self.docs
            }
        }
    };
}

documented!(SpecSection, name);
documented!(SpecCommand, name);
documented!(SpecFunction, name);
documented!(SpecEnum, name);

impl Documented for SpecKey {
    fn entry_name(&self) -> &str {
        &self.name
    }
    fn docs_mut(&mut self) -> &mut Docs {
        &mut self.docs
    }
    fn set_extra(&mut self, field: &str, value: &Value) {
        if let (true, Some(text)) = (field == "valueType", value.as_str()) {
            self.value_type = text.to_string();
        }
    }
}

impl Documented for SpecResourceMember {
    fn entry_name(&self) -> &str {
        &self.name
    }
    fn docs_mut(&mut self) -> &mut Docs {
        &mut self.docs
    }
    fn set_extra(&mut self, field: &str, value: &Value) {
        if let (true, Some(items)) = (field == "args", value.as_array()) {
            self.args = items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
        }
    }
}

impl Documented for SpecOperatorToken {
    fn entry_name(&self) -> &str {
        &self.token
    }
    fn docs_mut(&mut self) -> &mut Docs {
        &mut self.docs
    }
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn is_string_array(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(Value::is_str))
}

fn text_of(raw: &Table, field: &str) -> String {
    match raw.get(field) {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => "undefined".to_string(),
    }
}

fn apply_overlay(spec: &mut Spec, overlay: &Table, problems: &mut Problems) {
    for (group, entries) in overlay {
        let Some(entries) = entries.as_table() else {
            problems.add(group, "expected a table of entries");
            continue;
        };
        for (name, raw) in entries {
            let path = format!("{group}.{name}");
            let Some(raw) = raw.as_table() else {
                problems.add(&path, "expected a table");
                continue;
            };
            match group.as_str() {
                "sections" => merge_section(spec, name, raw, &path, problems),
                "commands" => merge_command(spec, name, raw, &path, problems),
                "members" => merge_member(spec, name, raw, &path, problems),
                "functions" => merge_function(spec, name, raw, &path, problems),
                "enums" => merge_enum(spec, name, raw, &path, problems),
                "operators" => merge_operator(spec, name, raw, &path, problems),
                _ => problems.add(group, "unknown overlay group"),
            }
        }
    }
}

fn merge_command(spec: &mut Spec, name: &str, raw: &Table, path: &str, problems: &mut Problems) {
    merge_named(
        &mut spec.body.commands,
        name,
        raw,
        path,
        problems,
        &[],
        |problems| {
            let (xxmi, docs) = manual_entry(raw, path, problems, &["kind"])?;
            Some(SpecCommand {
                name: name.to_string(),
                kind: text_of(raw, "kind"),
                match_kind: crate::MatchKind::Exact,
                values: Vec::new(),
                source: "manual".to_string(),
                xxmi,
                arg_count: None,
                indirect: None,
                form: None,
                docs,
            })
        },
    );
}

fn merge_member(spec: &mut Spec, name: &str, raw: &Table, path: &str, problems: &mut Problems) {
    merge_named(
        &mut spec.body.resource_members,
        name,
        raw,
        path,
        problems,
        &["args"],
        |problems| {
            let (xxmi, docs) = manual_entry(raw, path, problems, &["target"])?;
            Some(SpecResourceMember {
                name: name.to_string(),
                target: text_of(raw, "target"),
                args: Vec::new(),
                source: "manual".to_string(),
                xxmi,
                docs,
            })
        },
    );
}

fn merge_function(spec: &mut Spec, name: &str, raw: &Table, path: &str, problems: &mut Problems) {
    merge_named(
        &mut spec.body.functions,
        name,
        raw,
        path,
        problems,
        &[],
        |problems| {
            let (xxmi, docs) = manual_entry(raw, path, problems, &[])?;
            Some(SpecFunction {
                name: name.to_string(),
                source: "manual".to_string(),
                xxmi,
                docs,
            })
        },
    );
}

fn merge_enum(spec: &mut Spec, name: &str, raw: &Table, path: &str, problems: &mut Problems) {
    merge_named(
        &mut spec.body.enums,
        name,
        raw,
        path,
        problems,
        &[],
        |problems| {
            let (xxmi, docs) = manual_entry(raw, path, problems, &[])?;
            Some(SpecEnum {
                name: name.to_string(),
                values: Vec::new(),
                source: "manual".to_string(),
                xxmi,
                docs,
            })
        },
    );
}

/// Operator tokens are matched exactly, unlike every other group.
fn merge_operator(spec: &mut Spec, name: &str, raw: &Table, path: &str, problems: &mut Problems) {
    let tokens = &mut spec.body.operators.tokens;
    if let Some(token) = tokens.iter_mut().find(|t| t.token == name) {
        assign(token, raw, path, problems, &[]);
    } else if let Some((xxmi, docs)) = manual_entry(raw, path, problems, &[]) {
        tokens.push(SpecOperatorToken {
            token: name.to_string(),
            source: "manual".to_string(),
            xxmi,
            docs,
        });
    }
}

/// The index of the section `name`, added from the overlay when the spec has none.
fn section_index(
    spec: &mut Spec,
    name: &str,
    fields: &Table,
    path: &str,
    problems: &mut Problems,
) -> Option<usize> {
    if let Some(at) = spec.body.sections.iter().position(|s| same(&s.name, name)) {
        assign(&mut spec.body.sections[at], fields, path, problems, &[]);
        return Some(at);
    }

    let (xxmi, docs) = manual_entry(fields, path, problems, &["kind"])?;
    spec.body.sections.push(SpecSection {
        name: name.to_string(),
        prefix: fields.get("prefix") == Some(&Value::Boolean(true)),
        kind: text_of(fields, "kind"),
        allows_bare_lines: fields.get("allowsBareLines") == Some(&Value::Boolean(true)),
        allows_duplicate_keys: fields.get("allowsDuplicateKeys") == Some(&Value::Boolean(true)),
        keys: Vec::new(),
        removed_keys: Vec::new(),
        dynamic_keys: Vec::new(),
        source: "manual".to_string(),
        xxmi,
        docs,
    });
    Some(spec.body.sections.len() - 1)
}

fn merge_section(spec: &mut Spec, name: &str, raw: &Table, path: &str, problems: &mut Problems) {
    let mut fields = raw.clone();
    let keys = fields.remove("keys");
    let Some(at) = section_index(spec, name, &fields, path, problems) else {
        return;
    };

    let Some(keys) = keys else { return };
    let Some(keys) = keys.as_table() else {
        problems.add(&format!("{path}.keys"), "expected a table of keys");
        return;
    };
    for (key_name, key_raw) in keys {
        let key_path = format!("{path}.keys.{key_name}");
        let Some(key_raw) = key_raw.as_table() else {
            problems.add(&key_path, "expected a table");
            continue;
        };
        merge_named(
            &mut spec.body.sections[at].keys,
            key_name,
            key_raw,
            &key_path,
            problems,
            &["valueType"],
            |problems| {
                let (xxmi, docs) = manual_entry(key_raw, &key_path, problems, &["valueType"])?;
                Some(SpecKey {
                    name: key_name.to_string(),
                    value_type: text_of(key_raw, "valueType"),
                    source: "manual".to_string(),
                    xxmi,
                    repeatable: None,
                    docs,
                })
            },
        );
    }
}

fn merge_named<T: Documented>(
    list: &mut Vec<T>,
    name: &str,
    raw: &Table,
    path: &str,
    problems: &mut Problems,
    extra_fields: &[&str],
    create: impl FnOnce(&mut Problems) -> Option<T>,
) {
    if let Some(entry) = list.iter_mut().find(|e| same(e.entry_name(), name)) {
        assign(entry, raw, path, problems, extra_fields);
    } else if let Some(added) = create(problems) {
        list.push(added);
    }
}

/// Copies documentation (and `extra_fields`) from `raw` onto an existing generated entry.
fn assign<T: Documented>(
    target: &mut T,
    raw: &Table,
    path: &str,
    problems: &mut Problems,
    extra_fields: &[&str],
) {
    for (field, value) in raw {
        let is_extra = extra_fields.contains(&field.as_str());
        if let Some(message) = field_problem(field, value, is_extra) {
            problems.add(path, &message);
        } else if is_extra {
            target.set_extra(field, value);
        } else if let Some(text) = value.as_str() {
            target.docs_mut().set(field, text.to_string());
        }
    }
}

/// Why an overlay field can't be applied, if it can't.
fn field_problem(field: &str, value: &Value, is_extra: bool) -> Option<String> {
    if field == "source" {
        return Some("`source` can only be set on new (`source = \"manual\"`) entries".into());
    }
    if !is_extra && !DOC_FIELDS.contains(&field) {
        return Some(format!("field `{field}` can't be set by an overlay"));
    }

    let is_args = field == "args";
    let valid = if is_args {
        is_string_array(value)
    } else {
        value.is_str()
    };
    if valid {
        return None;
    }
    let expected = if is_args {
        "an array of strings"
    } else {
        "a string"
    };
    Some(format!("`{field}` must be {expected}"))
}

/// The `xxmi` flag and documentation of an entry that only exists in the overlay;
/// `None` (with problems) if it isn't allowed.
fn manual_entry(
    raw: &Table,
    path: &str,
    problems: &mut Problems,
    required: &[&str],
) -> Option<(bool, Docs)> {
    if raw.get("source").and_then(Value::as_str) != Some("manual") {
        problems.add(
            path,
            "not in the generated spec (add `source = \"manual\"` and a `note` if intended)",
        );
        return None;
    }

    let mut ok = true;
    if raw
        .get("note")
        .and_then(Value::as_str)
        .is_none_or(|note| crate::text::trim(note).is_empty())
    {
        problems.add(path, "manual entries need a `note` saying why they exist");
        ok = false;
    }
    for field in required {
        if !raw.contains_key(*field) {
            problems.add(path, &format!("manual entries of this kind need `{field}`"));
            ok = false;
        }
    }
    if !ok {
        return None;
    }

    let mut docs = Docs::default();
    for field in DOC_FIELDS {
        if let Some(text) = raw.get(field).and_then(Value::as_str) {
            docs.set(field, text.to_string());
        }
    }
    Some((raw.get("xxmi") == Some(&Value::Boolean(true)), docs))
}
