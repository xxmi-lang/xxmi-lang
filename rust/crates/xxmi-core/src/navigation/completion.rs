//! Completion items from the cursor's context on its line.

use std::collections::HashMap;
use std::path::Path;

use crate::diagnostic::{Position, Range};
use crate::model::{IniFile, Scope};
use crate::spec::{CommandKind, MatchKind, SectionKind, SpecCommand};
use crate::workspace::{FileId, Workspace};

use super::markdown::docs;
use super::text::{
    enum_table, is_word, js_trim_start, line_of, suffix_where, utf16_len, utf16_prefix,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Section,
    Key,
    Command,
    Keyword,
    Value,
    Variable,
    Reference,
    File,
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    /// The range the item replaces (the word being typed).
    pub range: Range,
    /// Text to insert when it isn't the label (`hash = ` for the key `hash`).
    pub insert_text: Option<String>,
}

impl CompletionItem {
    fn new(label: impl Into<String>, kind: CompletionKind, range: Range) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            documentation: None,
            range,
            insert_text: None,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// A JavaScript `Map` of items: a repeated key replaces the item but keeps its place.
#[derive(Default)]
struct Items {
    order: Vec<String>,
    items: HashMap<String, CompletionItem>,
}

impl Items {
    fn set(&mut self, key: String, item: CompletionItem) {
        if self.items.insert(key.clone(), item).is_none() {
            self.order.push(key);
        }
    }

    fn add_new(&mut self, key: String, item: CompletionItem) {
        if !self.items.contains_key(&key) {
            self.set(key, item);
        }
    }

    fn into_vec(mut self) -> Vec<CompletionItem> {
        self.order
            .iter()
            .filter_map(|key| self.items.remove(key))
            .collect()
    }
}

/// Section prefixes a reference value starts with.
const REFERENCE_PREFIXES: [&str; 6] = [
    "CommandList",
    "CustomShader",
    "BuiltInCommandList",
    "BuiltInCustomShader",
    "Resource",
    "Pool",
];

/// The line before the cursor, and where on the line the cursor is.
struct Cursor<'a> {
    pos: Position,
    line: &'a str,
    before: &'a str,
}

impl Cursor<'_> {
    /// From `start` (a column) to the cursor.
    fn range(&self, start: u32) -> Range {
        Range {
            start: Position {
                line: self.pos.line,
                character: start,
            },
            end: self.pos,
        }
    }

    /// The range of the `[...]*$` match of `accept` at the end of the text before the cursor.
    fn word_range(&self, accept: impl Fn(char) -> bool) -> Range {
        let word = suffix_where(self.before, accept);
        self.range(self.pos.character - utf16_len(word))
    }
}

/// Completions at the cursor, from its context on the line.
pub fn completions(workspace: &Workspace, id: FileId, pos: Position) -> Vec<CompletionItem> {
    let file = &workspace.files()[id];
    let (line, line_start) = line_of(&file.text, pos.line);
    let before = utf16_prefix(line, pos.character);
    let cursor = Cursor { pos, line, before };

    if js_trim_start(before).starts_with('[') && !before.contains(']') {
        return section_kinds(workspace, &cursor);
    }
    // The DLL only treats `;` at the start of a line as a comment.
    if js_trim_start(before).starts_with(';') {
        return Vec::new();
    }

    let offset = line_start + before.len();
    let section = file.sections.iter().rposition(|s| s.chunk.start <= offset);
    let spec = section.and_then(|index| file.sections[index].spec);

    if dollar_start(before).is_some() {
        return variable_items(workspace, file, section, &cursor);
    }
    match before.find('=') {
        None => key_items(workspace, spec, &cursor),
        Some(eq) => value_items(workspace, file, spec, &cursor, eq),
    }
}

fn section_kinds(workspace: &Workspace, cursor: &Cursor) -> Vec<CompletionItem> {
    let start = cursor
        .before
        .find('[')
        .map_or(0, |at| utf16_len(&cursor.before[..at]) + 1);
    let range = cursor.range(start);

    workspace
        .lookup()
        .spec()
        .sections
        .iter()
        .map(|s| {
            let kind = if s.kind == SectionKind::CommandList {
                "command-list section"
            } else {
                "section"
            };
            let prefix = if s.prefix { " (prefix)" } else { "" };
            let xxmi = if s.meta.xxmi { " · XXMI-only" } else { "" };
            let mut item = CompletionItem::new(&s.name, CompletionKind::Section, range)
                .detail(format!("{kind}{prefix}{xxmi}"));
            item.documentation = Some(docs(&s.meta));
            item
        })
        .collect()
}

/// Where the trailing `$name` of the text starts.
fn dollar_start(before: &str) -> Option<usize> {
    let word = suffix_where(before, |c| is_word(c) || c == '\\');
    let start = before.len() - word.len();
    start
        .checked_sub(1)
        .filter(|&at| before.as_bytes()[at] == b'$')
}

fn variable_items(
    workspace: &Workspace,
    file: &IniFile,
    section: Option<usize>,
    cursor: &Cursor,
) -> Vec<CompletionItem> {
    let start = dollar_start(cursor.before).unwrap_or(cursor.before.len());
    let range = cursor.range(cursor.pos.character - utf16_len(&cursor.before[start..]));
    let mut items = Items::default();

    for variable in &file.variables {
        if variable.scope == Scope::Local && variable.section == section {
            let item = CompletionItem::new(&variable.name, CompletionKind::Variable, range)
                .detail("local");
            items.set(variable.name.to_lowercase(), item);
        }
    }

    let own = file.namespace.to_lowercase();
    for variable in workspace.symbols().all_variables() {
        // `$\ns\name`: offer the short form in the file's own namespace.
        let short = match variable.key.strip_prefix(&format!("$\\{own}\\")) {
            Some(rest) if !own.is_empty() => format!("${rest}"),
            _ => variable.key.clone(),
        };
        let flags = [
            Some("global"),
            variable.persist.then_some("persist"),
            variable.locked.then_some("locked"),
        ];
        let flags: Vec<&str> = flags.into_iter().flatten().collect();
        let item =
            CompletionItem::new(&short, CompletionKind::Variable, range).detail(flags.join(" "));
        items.add_new(short, item);
    }
    items.into_vec()
}

/// A key replaces the whole word and gets ` = ` unless the line already has one.
fn key_items(
    workspace: &Workspace,
    spec: Option<&'static crate::spec::SpecSection>,
    cursor: &Cursor,
) -> Vec<CompletionItem> {
    let Some(spec) = spec else {
        return Vec::new();
    };
    let range = cursor.word_range(|c| is_word(c) || c == '-');
    let assign = if cursor.line.contains('=') { "" } else { " = " };

    let mut items: Vec<CompletionItem> = spec
        .keys
        .iter()
        .map(|key| {
            let xxmi = if key.meta.xxmi { " · XXMI-only" } else { "" };
            let mut item = CompletionItem::new(&key.name, CompletionKind::Key, range)
                .detail(format!("{}{xxmi}", key.value_type));
            item.documentation = Some(docs(&key.meta));
            if !assign.is_empty() {
                item.insert_text = Some(format!("{}{assign}", key.name));
            }
            item
        })
        .collect();

    if spec.kind == SectionKind::CommandList {
        items.extend(
            workspace
                .lookup()
                .key_commands()
                .filter_map(|command| command_item(command, range, assign)),
        );
    }
    items
}

fn command_item(command: &SpecCommand, range: Range, assign: &str) -> Option<CompletionItem> {
    let keyword = matches!(command.kind, CommandKind::Flow | CommandKind::Declaration);
    let by_prefix = command.match_kind == MatchKind::Prefix;
    // `pre`/`post` and the other prefixes only make sense in front of something else.
    if command.kind == CommandKind::Prefix || (by_prefix && !keyword) {
        return None;
    }

    // Flow keywords and declarations are followed by an expression, not `=`.
    let insert_text = if keyword {
        (by_prefix || command.kind == CommandKind::Declaration)
            .then(|| format!("{} ", command.name))
    } else {
        (!assign.is_empty()).then(|| format!("{}{assign}", command.name))
    };
    let kind = if keyword {
        CompletionKind::Keyword
    } else {
        CompletionKind::Command
    };
    let xxmi = if command.meta.xxmi {
        " · XXMI-only"
    } else {
        ""
    };

    let mut item = CompletionItem::new(&command.name, kind, range)
        .detail(format!("{}{xxmi}", command.kind.as_str()));
    item.documentation = Some(docs(&command.meta));
    item.insert_text = insert_text;
    Some(item)
}

fn value_items(
    workspace: &Workspace,
    file: &IniFile,
    spec: Option<&'static crate::spec::SpecSection>,
    cursor: &Cursor,
    eq: usize,
) -> Vec<CompletionItem> {
    let lookup = workspace.lookup();
    let raw_key = cursor.before[..eq].trim().to_lowercase();
    let key = strip_list_selector(&raw_key);
    let value_before = &cursor.before[eq + 1..];

    if spec.is_some() && matches!(key, "filename" | "include" | "include_recursive") {
        return file_items(workspace, file, value_before, cursor);
    }

    let key_spec = spec.and_then(|s| lookup.key(s, key));
    if let Some(table) = key_spec.and_then(|k| enum_table(&k.value_type)) {
        let range = cursor.word_range(is_word);
        let values = lookup
            .enum_table(table)
            .map(|t| t.values.as_slice())
            .unwrap_or_default();
        return values
            .iter()
            .map(|v| {
                CompletionItem::new(&v.name, CompletionKind::Value, range)
                    .detail(format!("enum:{table}"))
            })
            .collect();
    }

    // A reference, also while its prefix is still being typed (`Res` -> `ResourceFoo`).
    let word = suffix_where(value_before, |c| is_word(c) || c == '.' || c == '\\');
    let word_start = cursor.pos.character - utf16_len(word);
    let lower = word.to_lowercase();
    let typing_reference = !word.is_empty()
        && REFERENCE_PREFIXES.iter().any(|p| {
            let prefix = p.to_lowercase();
            lower.starts_with(&prefix) || prefix.starts_with(&lower)
        });
    if typing_reference || key == "run" || key == "checktextureoverride" {
        return reference_items(
            workspace,
            file,
            cursor.range(word_start),
            key == "run",
            None,
        );
    }

    match spec {
        Some(spec) if spec.kind == SectionKind::CommandList => {
            command_list_value_items(workspace, file, spec, key, (value_before, word), cursor)
        }
        _ => Vec::new(),
    }
}

/// `pre `/`post ` in front of a command-list key.
fn strip_list_selector(key: &str) -> &str {
    for selector in ["pre", "post"] {
        if let Some(rest) = key.strip_prefix(selector)
            && rest.starts_with(char::is_whitespace)
        {
            return rest.trim_start();
        }
    }
    key
}

fn command_list_value_items(
    workspace: &Workspace,
    file: &IniFile,
    spec: &'static crate::spec::SpecSection,
    key: &str,
    (value_before, word): (&str, &str),
    cursor: &Cursor,
) -> Vec<CompletionItem> {
    let lookup = workspace.lookup();
    let command = lookup.command(key);

    // Lines that are neither keys nor commands are resource copies (`ps-t0 = ResourceFoo`,
    // ParseCommandListResourceCopyDirective): offer resources and pools.
    if command.is_none() && lookup.key(spec, key).is_none() && value_before.trim() == word {
        let range = cursor.range(cursor.pos.character - utf16_len(word));
        return reference_items(workspace, file, range, false, Some(&["resource", "pool"]));
    }

    let Some(command) = command.filter(|c| !c.values.is_empty()) else {
        return Vec::new();
    };
    let range = cursor.word_range(is_word);
    command
        .values
        .iter()
        .map(|v| {
            CompletionItem::new(&v.name, CompletionKind::Value, range)
                .detail(format!("{} value", command.name))
        })
        .collect()
}

fn reference_items(
    workspace: &Workspace,
    file: &IniFile,
    range: Range,
    callable: bool,
    kinds: Option<&[&str]>,
) -> Vec<CompletionItem> {
    let kinds: &[&str] = kinds.unwrap_or(if callable {
        &["commandlist", "customshader"]
    } else {
        &["commandlist", "customshader", "resource", "pool"]
    });
    let lookup = workspace.lookup();
    let own = file.namespace.to_lowercase();
    let mut items = Items::default();

    for symbol in workspace.symbols().all_sections() {
        if !kinds.contains(&symbol.kind.as_str()) {
            continue;
        }
        let q = &symbol.qualified_name;
        let prefix = lookup.section_prefix(q).unwrap_or("");
        let own_prefix = format!("{}\\{own}\\", prefix.to_lowercase());
        let in_own = !own.is_empty() && q.to_lowercase().starts_with(&own_prefix);
        let label = if in_own {
            format!(
                "{prefix}{}",
                q.get(prefix.len() + own.len() + 2..).unwrap_or("")
            )
        } else {
            q.clone()
        };
        let item = CompletionItem::new(&label, CompletionKind::Reference, range)
            .detail(symbol.kind.as_str());
        items.add_new(label.to_lowercase(), item);
    }

    // Built-in sections are all command lists and shaders.
    if kinds.contains(&"commandlist") {
        for builtin in &lookup.spec().builtin_sections {
            let item = CompletionItem::new(&builtin.name, CompletionKind::Reference, range)
                .detail("built into the DLL");
            items.add_new(builtin.name.to_lowercase(), item);
        }
    }
    items.into_vec()
}

fn file_items(
    workspace: &Workspace,
    file: &IniFile,
    value_before: &str,
    cursor: &Cursor,
) -> Vec<CompletionItem> {
    let typed = value_before.trim_start();
    let slash = typed.rfind(['/', '\\']);
    let dir_part = slash.map_or("", |at| &typed[..at]);
    let dir = file.path.parent().unwrap_or(Path::new(""));

    let base = if dir_part.is_empty() {
        Some(dir.to_path_buf())
    } else {
        workspace.resolver().resolve(dir, dir_part)
    };
    let Some(base) = base else {
        return Vec::new();
    };

    let tail = slash.map_or(typed, |at| &typed[at + 1..]);
    let range = cursor.range(cursor.pos.character - utf16_len(tail));
    let names = workspace.resolver().list(&base);
    names
        .iter()
        .flat_map(|names| names.iter())
        .filter(|name| !name.starts_with('.'))
        .map(|name| CompletionItem::new(name, CompletionKind::File, range))
        .collect()
}
