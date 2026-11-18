//! Just enough C++ reading to pull tables and call sites out of the DLL source. Everything is
//! located by symbol name, never by line number, so the extractor survives source drift.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use regex::{Match, Regex};

use crate::text::{is_space, is_word_byte, prefix_units, trim, utf16_len};

/// A symbol the extractor expects in the DLL source is gone.
#[derive(Debug, Clone)]
pub struct AnchorNotFound {
    pub file: String,
    pub symbol: String,
    message: String,
}

impl AnchorNotFound {
    pub fn new(file: &str, symbol: &str, detail: &str) -> Self {
        let suffix = if detail.is_empty() {
            String::new()
        } else {
            format!(" ({detail})")
        };
        Self {
            file: file.to_string(),
            symbol: symbol.to_string(),
            message: format!("{file}: could not find {symbol}{suffix}"),
        }
    }
}

impl fmt::Display for AnchorNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AnchorNotFound {}

thread_local! {
    static REGEXES: RefCell<HashMap<String, Regex>> = RefCell::new(HashMap::new());
}

/// Patterns built from symbol names repeat across files and sections; compile each once.
pub fn cached_regex(pattern: &str) -> Regex {
    REGEXES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(regex) = cache.get(pattern) {
            return regex.clone();
        }
        let regex = Regex::new(pattern).expect("extractor patterns are valid");
        cache.insert(pattern.to_string(), regex.clone());
        regex
    })
}

/// A source file with comments blanked out (same length, newlines kept) for scanning.
#[derive(Clone)]
pub struct CppFile {
    pub name: String,
    pub code: String,
}

static QUALIFIER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:const|override|noexcept)(?-u:\b)").unwrap());
static ENUM_TABLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"EnumName_t(?-u:\s)*<[^>]*>(?-u:\s)*([A-Za-z0-9_]+)(?-u:\s)*\[(?-u:\s)*\](?-u:\s)*=(?-u:\s)*\{")
        .unwrap()
});

impl CppFile {
    pub fn new(name: impl Into<String>, text: &str) -> Self {
        Self {
            name: name.into(),
            code: strip_comments(text),
        }
    }

    /// Body (between the braces, exclusive) of the definition of `name`, e.g. `Foo::Bar`.
    pub fn function_body(&self, name: &str) -> Result<&str, AnchorNotFound> {
        self.try_function_body(name)
            .ok_or_else(|| AnchorNotFound::new(&self.name, &format!("function {name}"), ""))
    }

    pub fn try_function_body(&self, name: &str) -> Option<&str> {
        let pattern = cached_regex(&format!(r"{}(?-u:\s)*\(", regex::escape(name)));
        for m in unprefixed_matches(&pattern, &self.code, b"._>:") {
            let open = m.end() - 1;
            let Some(close) = match_bracket(&self.code, open) else {
                continue;
            };
            let mut i = skip_space(&self.code, close + 1);
            // Skip trailing qualifiers such as `const` or `override`.
            while let Some(q) = QUALIFIER.find(&self.code[i..]) {
                i = skip_space(&self.code, i + q.end());
            }
            if self.code.as_bytes().get(i) != Some(&b'{') {
                continue;
            }
            let Some(end) = match_bracket(&self.code, i) else {
                continue;
            };
            return Some(&self.code[i + 1..end]);
        }
        None
    }

    /// Contents of the initializer of `name[] = { … }` (exclusive of the outer braces).
    pub fn array_initializer(&self, name: &str) -> Result<&str, AnchorNotFound> {
        self.try_array_initializer(name)
            .ok_or_else(|| AnchorNotFound::new(&self.name, &format!("array {name}[]"), ""))
    }

    pub fn try_array_initializer(&self, name: &str) -> Option<&str> {
        let pattern = cached_regex(&format!(
            r"(?-u:\b){}(?-u:\s)*\[(?-u:\s)*\](?-u:\s)*=(?-u:\s)*\{{",
            regex::escape(name)
        ));
        let m = pattern.find(&self.code)?;
        let open = m.end() - 1;
        let close = match_bracket(&self.code, open)?;
        Some(&self.code[open + 1..close])
    }

    /// Replacement text of a (possibly multi-line) `#define NAME …`.
    pub fn macro_text(&self, name: &str) -> Option<String> {
        let lines: Vec<&str> = self
            .code
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .collect();
        let define = cached_regex(&format!(
            r"^[ \t]*#define[ \t]+{}(?-u:\b)(.*)$",
            regex::escape(name)
        ));
        let (start, first) = lines.iter().enumerate().find_map(|(i, l)| {
            define
                .captures(l)
                .map(|c| (i, c.get(1).map_or("", |m| m.as_str())))
        })?;
        let mut body: Vec<&str> = Vec::new();
        let mut line = first;
        let mut i = start + 1;
        loop {
            let trimmed = line.trim_end_matches(is_space);
            let continued = trimmed.ends_with('\\');
            body.push(if continued {
                &trimmed[..trimmed.len() - 1]
            } else {
                line
            });
            if !continued || i >= lines.len() {
                break;
            }
            line = lines[i];
            i += 1;
        }
        Some(body.join("\n"))
    }

    /// Names of all `EnumName_t<…> NameNames[] = {` tables in the file.
    pub fn enum_table_names(&self) -> Vec<&str> {
        ENUM_TABLE
            .captures_iter(&self.code)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect()
    }
}

/// Matches of `pattern` that aren't directly preceded by one of `forbidden` (the lookbehind
/// the regex crate lacks); a rejected match resumes one character later, like a regex engine.
fn unprefixed_matches<'h>(pattern: &Regex, code: &'h str, forbidden: &[u8]) -> Vec<Match<'h>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at <= code.len() {
        let Some(m) = pattern.find_at(code, at) else {
            break;
        };
        let prefixed = m.start() > 0 && {
            let before = code.as_bytes()[m.start() - 1];
            is_word_byte(before) || forbidden.contains(&before)
        };
        if prefixed {
            at = next_char_boundary(code, m.start());
        } else {
            at = m.end();
            found.push(m);
        }
    }
    found
}

fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut next = i + 1;
    while next < s.len() && !s.is_char_boundary(next) {
        next += 1;
    }
    next
}

#[cfg(test)]
mod tests;

/// Blanks out line and block comments (one space per UTF-16 unit, newlines kept), leaving string
/// and char literals intact.
pub fn strip_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        if c == b'"' || c == b'\'' {
            let end = literal_end(bytes, i);
            out.push_str(&text[i..end]);
            i = end;
        } else if c == b'/' && next == Some(b'/') {
            let end = text[i..].find('\n').map_or(text.len(), |p| i + p);
            push_spaces(&mut out, utf16_len(&text[i..end]));
            i = end;
        } else if c == b'/' && next == Some(b'*') {
            let stop = text[i + 2..]
                .find("*/")
                .map_or(text.len(), |p| i + 2 + p + 2);
            blank_block(&mut out, &text[i..stop]);
            i = stop;
        } else {
            let run = bytes[i + 1..]
                .iter()
                .position(|b| matches!(b, b'"' | b'\'' | b'/'))
                .map_or(text.len(), |p| i + 1 + p);
            out.push_str(&text[i..run]);
            i = run;
        }
    }
    out
}

fn push_spaces(out: &mut String, count: usize) {
    out.extend(std::iter::repeat_n(' ', count));
}

fn blank_block(out: &mut String, comment: &str) {
    for c in comment.chars() {
        if c == '\n' {
            out.push('\n');
        } else {
            push_spaces(out, c.len_utf16());
        }
    }
}

/// Index just past the string or char literal starting at `start`.
fn literal_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
        i += if bytes[i] == b'\\' { 2 } else { 1 };
    }
    (i + 1).min(bytes.len())
}

/// Index of the bracket closing the one at `open`, skipping literals; `None` if unbalanced.
pub fn match_bracket(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut stack: Vec<u8> = Vec::new();
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            i = literal_end(bytes, i);
            continue;
        }
        match c {
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b'{' => stack.push(b'}'),
            _ if stack.last() == Some(&c) => {
                stack.pop();
                if stack.is_empty() {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_space(code: &str, i: usize) -> usize {
    let rest = &code[i..];
    rest.find(|c| !is_space(c)).map_or(code.len(), |p| i + p)
}

/// Splits at top-level commas (outside brackets and literals); trims each part.
pub fn split_top_level(code: &str) -> Vec<&str> {
    let bytes = code.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                i = literal_end(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(trim(&code[start..i]));
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = trim(&code[start..]);
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

const JS_ANY: &str = r"[^\n\r\u{2028}\u{2029}]";

static LITERAL_EXPR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"^L?"((?:[^"\\]|\\{JS_ANY})*)"$"#)).unwrap());
static LITERALS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"L?"((?:[^"\\\n]|\\{JS_ANY})*)""#)).unwrap());
static ESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\\({JS_ANY})")).unwrap());

fn unescape(raw: &str) -> String {
    ESCAPE.replace_all(raw, "$1").into_owned()
}

/// The value of a `L"…"` or `"…"` literal expression, or `None` if it isn't one.
pub fn string_literal(expr: &str) -> Option<String> {
    let caps = LITERAL_EXPR.captures(trim(expr))?;
    Some(unescape(&caps[1]))
}

/// All string literal values in `code`, in order.
pub fn string_literals(code: &str) -> Vec<String> {
    LITERALS
        .captures_iter(code)
        .map(|c| unescape(&c[1]))
        .collect()
}

pub struct Call<'a> {
    pub args: Vec<&'a str>,
    /// Offset of the call within the scanned code.
    pub index: usize,
}

impl Call<'_> {
    /// The argument at `i`, or `""` when the call has fewer.
    pub fn arg(&self, i: usize) -> &str {
        self.args.get(i).copied().unwrap_or("")
    }
}

/// Every call `name(…)` in `code`, with its top-level arguments.
pub fn find_calls<'a>(code: &'a str, name: &str) -> Vec<Call<'a>> {
    let pattern = cached_regex(&format!(
        r"{}(?-u:\s)*(?:<[^;(]*>)?(?-u:\s)*\(",
        regex::escape(name)
    ));
    let mut calls = Vec::new();
    for m in unprefixed_matches(&pattern, code, b"._>") {
        let open = m.end() - 1;
        let Some(close) = match_bracket(code, open) else {
            continue;
        };
        calls.push(Call {
            args: split_top_level(&code[open + 1..close]),
            index: m.start(),
        });
    }
    calls
}

pub struct IfBlock<'a> {
    pub condition: &'a str,
    /// The statement or `{…}` block the condition guards, up to the next top-level `else`.
    pub body: &'a str,
}

/// Top-level `if (cond) body` statements of a function body, including each `else if` link.
/// Good enough for the DLL's dispatch functions, which are flat if-chains.
pub fn if_blocks(code: &str) -> Vec<IfBlock<'_>> {
    let bytes = code.as_bytes();
    let mut blocks = Vec::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            i = literal_end(bytes, i);
            continue;
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
        }
        if depth != 0 || !starts_if(code, i) || (i > 0 && is_word_byte(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let Some(open) = code[i..].find('(').map(|p| i + p) else {
            break;
        };
        let Some(close) = match_bracket(code, open) else {
            break;
        };
        let body_start = skip_space(code, close + 1);
        let body_end = if bytes.get(body_start) == Some(&b'{') {
            match match_bracket(code, body_start) {
                Some(end) => end + 1,
                None => break,
            }
        } else {
            statement_end(code, body_start)
        };
        blocks.push(IfBlock {
            condition: &code[open + 1..close],
            body: &code[body_start..body_end],
        });
        i = body_end;
    }
    blocks
}

/// `if`, optional space, `(` within the next four characters: wider gaps don't count.
fn starts_if(code: &str, i: usize) -> bool {
    static IF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^if(?-u:\s)*\(").unwrap());
    code.as_bytes()[i] == b'i' && IF.is_match(prefix_units(&code[i..], 4))
}

/// Index after the `;` ending the statement starting at `start`.
fn statement_end(code: &str, start: usize) -> usize {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                i = literal_end(bytes, i);
                continue;
            }
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b';' if depth == 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    code.len()
}
