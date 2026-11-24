//! Hover for the symbol, key, command, operator or section under the cursor.

use crate::diagnostic::{Position, Range};
use crate::model::{IniFile, LineModel, SectionModel};
use crate::scan::Span;
use crate::spec::{MatchKind, SectionKind, SpecCommand, SpecKey, SpecSection};
use crate::workspace::symbols::{Origin, SectionSymbol, VariableSymbol};
use crate::workspace::{FileId, Workspace};

use super::markdown::{badge, doc_comment, docs, origin_text, source};
use super::target::{Symbol, Target, contains, target_at};
use super::text::{enum_table, is_word};

pub struct Hover {
    /// Markdown.
    pub contents: String,
    pub range: Range,
}

pub fn hover(workspace: &Workspace, id: FileId, pos: Position) -> Option<Hover> {
    let file = &workspace.files()[id];
    let offset = file.offset_at(pos);

    match target_at(workspace, id, pos) {
        Some(Target::Symbol {
            symbol: Symbol::Section(symbol),
            range,
        }) => {
            return Some(section_symbol_hover(workspace, &symbol, range));
        }
        Some(Target::Symbol {
            symbol: Symbol::Variable(symbol),
            range,
        }) => {
            return Some(variable_symbol_hover(workspace, &symbol, range));
        }
        _ => {}
    }
    syntax_hover(workspace, file, pos, offset)
}

fn join_parts(parts: &[Option<String>]) -> String {
    let present: Vec<&str> = parts
        .iter()
        .flatten()
        .map(String::as_str)
        .filter(|p| !p.is_empty())
        .collect();
    present.join("\n\n")
}

fn section_symbol_hover(workspace: &Workspace, symbol: &SectionSymbol, range: Range) -> Hover {
    let doc = match &symbol.origin {
        Origin::File {
            file,
            section: Some(index),
        } => {
            let file = &workspace.files()[*file];
            file.sections
                .get(*index)
                .and_then(|section| doc_comment(file, section.header.line, true))
        }
        _ => None,
    };
    let contents = join_parts(&[
        Some(format!("```ini\n[{}]\n```", symbol.qualified_name)),
        doc,
        Some(origin_text(&symbol.origin)),
    ]);
    Hover { contents, range }
}

fn variable_symbol_hover(workspace: &Workspace, symbol: &VariableSymbol, range: Range) -> Hover {
    let declaration = symbol.declaration.map(|(file, index)| {
        (
            &workspace.files()[file],
            &workspace.files()[file].variables[index],
        )
    });
    let scope = declaration.map_or("global", |(_, d)| match d.scope {
        crate::model::Scope::Local => "local",
        crate::model::Scope::Global => "global",
    });
    let flags: Vec<&str> = [
        Some(scope),
        symbol.persist.then_some("persist"),
        symbol.locked.then_some("locked"),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut initial = String::new();
    let mut doc = None;
    if let (Origin::File { file, .. }, Some((_, declaration))) = (&symbol.origin, declaration) {
        let file = &workspace.files()[*file];
        let line = file.range_of(declaration.span).start.line;
        let text = file
            .text
            .split('\n')
            .nth(line as usize)
            .map_or("", str::trim);
        if !text.is_empty() {
            initial = format!("```ini\n{text}\n```");
        }
        doc = doc_comment(file, line, false);
    }

    let name = declaration.map_or(symbol.key.as_str(), |(_, d)| d.name.as_str());
    let header = format!("**{}** `{name}`", flags.join(" "));
    let contents = join_parts(&[
        Some(header),
        Some(initial),
        doc,
        Some(origin_text(&symbol.origin)),
    ]);
    Hover { contents, range }
}

fn section_at(file: &IniFile, offset: usize) -> Option<&SectionModel> {
    file.sections.iter().find(|s| {
        contains(s.header.content, offset)
            || s.lines.iter().any(|l| contains(l.scanned.content, offset))
    })
}

fn syntax_hover(
    workspace: &Workspace,
    file: &IniFile,
    pos: Position,
    offset: usize,
) -> Option<Hover> {
    let section = section_at(file, offset)?;
    if let Some(spec) = section.spec
        && contains(section.name, offset)
    {
        return Some(Hover {
            contents: section_doc(spec),
            range: file.range_of(section.name),
        });
    }

    let line = section.lines.iter().find(|l| l.scanned.line == pos.line)?;
    let spec = section.spec?;
    let key_span = line.scanned.key.unwrap_or(line.scanned.content);
    if !contains(key_span, offset) {
        return value_hover(workspace, file, line, spec, pos, offset);
    }

    let key_range = file.range_of(key_span);
    let key_name = match line.scanned.key {
        Some(key) => file.slice(key),
        None => file
            .slice(line.scanned.content)
            .split(char::is_whitespace)
            .next()
            .unwrap_or(""),
    };
    let command_name = line.command.as_deref().unwrap_or(key_name);
    let is_command_list = spec.kind == SectionKind::CommandList;

    let lookup = workspace.lookup();
    if let Some(key) = lookup.key(
        spec,
        if is_command_list {
            command_name
        } else {
            key_name
        },
    ) {
        return Some(Hover {
            contents: key_doc(key, spec),
            range: key_range,
        });
    }
    if is_command_list && let Some(command) = lookup.command(command_name) {
        return Some(Hover {
            contents: command_doc(command),
            range: key_range,
        });
    }
    None
}

pub fn command_doc(command: &SpecCommand) -> String {
    let args = command.arg_count.map_or(String::new(), |count| {
        format!(" ({count} argument{})", if count == 1 { "" } else { "s" })
    });
    let values = if command.values.is_empty() {
        String::new()
    } else {
        let names: Vec<String> = command
            .values
            .iter()
            .map(|v| {
                format!(
                    "`{}{}`",
                    v.name,
                    if v.match_kind == MatchKind::Prefix {
                        "…"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        format!("\n\nSpecial values: {}", names.join(", "))
    };
    format!(
        "**{}** — {} command{args}{}\n\n{}{values}{}",
        command.name,
        command.kind.as_str(),
        badge(&command.meta),
        docs(&command.meta),
        source(&command.meta)
    )
}

fn section_label(spec: &SpecSection) -> String {
    format!("{}{}", spec.name, if spec.prefix { "…" } else { "" })
}

pub fn section_doc(spec: &SpecSection) -> String {
    let kind = if spec.kind == SectionKind::CommandList {
        "command-list section"
    } else {
        "section"
    };
    let text = format!(
        "**[{}]** — {kind}{}\n\n{}{}",
        section_label(spec),
        badge(&spec.meta),
        docs(&spec.meta),
        source(&spec.meta)
    );
    text.trim().to_string()
}

fn key_doc(key: &SpecKey, section: &SpecSection) -> String {
    let text = format!(
        "**{}** in [{}] — `{}`{}\n\n{}{}",
        key.name,
        section_label(section),
        key.value_type,
        badge(&key.meta),
        docs(&key.meta),
        source(&key.meta)
    );
    text.trim().to_string()
}

/// `[A-Za-z_]\w*` matches of `text` as (UTF-16 index, UTF-16 length, text).
fn words(text: &str) -> Vec<(u32, u32, &str)> {
    let mut found = Vec::new();
    let mut units = 0;
    let mut chars = text.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        let begin = units;
        units += ch.len_utf16() as u32;
        if !(ch.is_ascii_alphabetic() || ch == '_') {
            continue;
        }

        let mut end = start + ch.len_utf8();
        while let Some(&(next, word)) = chars.peek() {
            if !is_word(word) {
                break;
            }
            end = next + word.len_utf8();
            units += 1;
            chars.next();
        }
        found.push((begin, units - begin, &text[start..end]));
    }
    found
}

fn utf16_index_of(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if from > haystack.len() || needle.is_empty() {
        return None;
    }
    (from..=haystack.len().checked_sub(needle.len())?)
        .find(|&i| haystack[i..i + needle.len()] == *needle)
}

fn span_range(line: u32, start: u32, len: u32) -> Range {
    Range {
        start: Position {
            line,
            character: start,
        },
        end: Position {
            line,
            character: start + len,
        },
    }
}

/// Operator or function docs for a token in a value.
fn value_hover(
    workspace: &Workspace,
    file: &IniFile,
    line: &LineModel,
    spec: &'static SpecSection,
    pos: Position,
    offset: usize,
) -> Option<Hover> {
    let value: Span = line.scanned.value.filter(|v| contains(*v, offset))?;
    let value_range = file.range_of(value);
    let value_text = file.slice(value);
    let first_column = value_range.start.character;
    let column = pos.character.checked_sub(first_column)?;

    word_hover(workspace, line, spec, value_text, column, |index, len| {
        span_range(line.scanned.line, first_column + index, len)
    })
    .or_else(|| {
        operator_hover(workspace, value_text, column, |index, len| {
            span_range(line.scanned.line, first_column + index, len)
        })
    })
}

fn word_hover(
    workspace: &Workspace,
    line: &LineModel,
    spec: &'static SpecSection,
    value_text: &str,
    column: u32,
    range_of: impl Fn(u32, u32) -> Range,
) -> Option<Hover> {
    let lookup = workspace.lookup();
    let units: Vec<u16> = value_text.encode_utf16().collect();

    for (index, len, word) in words(value_text) {
        if column < index || column > index + len {
            continue;
        }
        let range = range_of(index, len);
        let lower = word.to_lowercase();

        if line.command.as_deref() == Some("run")
            && let Some(command) = lookup
                .value_commands()
                .find(|c| c.name.to_lowercase() == lower)
        {
            return Some(Hover {
                contents: command_doc(command),
                range,
            });
        }

        let function = lookup
            .spec()
            .functions
            .iter()
            .find(|f| f.name.to_lowercase() == lower);
        if let Some(function) = function
            && units.get((index + len) as usize) == Some(&u16::from(b'('))
        {
            let text = format!(
                "**{}(…)** — expression function{}\n\n{}{}",
                function.name,
                badge(&function.meta),
                docs(&function.meta),
                source(&function.meta)
            );
            return Some(Hover {
                contents: text.trim().to_string(),
                range,
            });
        }

        let key_name = line.command.as_deref().unwrap_or("");
        let key = lookup.key(spec, key_name);
        if let Some(table) = key.and_then(|k| enum_table(&k.value_type)) {
            let name = key.map_or("", |k| k.name.as_str());
            return Some(Hover {
                contents: format!("Value of `{name}`: one of `enum:{table}`"),
                range,
            });
        }
    }
    None
}

fn operator_hover(
    workspace: &Workspace,
    value_text: &str,
    column: u32,
    range_of: impl Fn(u32, u32) -> Range,
) -> Option<Hover> {
    let operators = &workspace.lookup().spec().operators;
    let units: Vec<u16> = value_text.encode_utf16().collect();

    let mut tokens: Vec<_> = operators.tokens.iter().collect();
    tokens.sort_by_key(|t| std::cmp::Reverse(t.token.len()));

    for token in tokens {
        let needle: Vec<u16> = token.token.encode_utf16().collect();
        let len = needle.len() as u32;
        let from = column.saturating_sub(len) as usize;
        let Some(at) = utf16_index_of(&units, &needle, from) else {
            continue;
        };
        let at = at as u32;
        if at > column || column > at + len {
            continue;
        }

        let level = operators
            .precedence
            .iter()
            .find(|p| p.operators.contains(&token.token));
        let precedence = level.map_or(String::new(), |level| {
            format!(
                "\n\nPrecedence level {} (`{}`, {}-associative{}); 0 binds tightest.",
                level.level,
                level.group,
                level.associativity.as_str(),
                if level.unary { ", unary" } else { "" }
            )
        });
        let text = format!(
            "**`{}`** operator{}{precedence}\n\n{}{}",
            token.token,
            badge(&token.meta),
            docs(&token.meta),
            source(&token.meta)
        );
        return Some(Hover {
            contents: text.trim().to_string(),
            range: range_of(at, len),
        });
    }
    None
}
