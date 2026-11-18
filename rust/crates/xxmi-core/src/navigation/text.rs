//! Small text helpers shared by hover and completion. Columns are UTF-16 code units, like LSP.

/// Length in UTF-16 code units.
pub fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// The first `units` UTF-16 code units of `text`, clamped to its length.
pub fn utf16_prefix(text: &str, units: u32) -> &str {
    let mut seen = 0;
    for (index, ch) in text.char_indices() {
        if seen >= units {
            return &text[..index];
        }
        seen += ch.len_utf16() as u32;
    }
    text
}

/// The zero-based line `line` of `text` without its `\r\n`, and the byte offset it starts at.
pub fn line_of(text: &str, line: u32) -> (&str, usize) {
    let mut start = 0;
    for (index, content) in text.split('\n').enumerate() {
        if index == line as usize {
            return (content.strip_suffix('\r').unwrap_or(content), start);
        }
        start += content.len() + 1;
    }
    ("", text.len())
}

/// `trim_start` with JavaScript's `\s`, which also counts a byte order mark as whitespace.
pub fn js_trim_start(text: &str) -> &str {
    text.trim_start_matches(|ch: char| ch.is_whitespace() || ch == '\u{feff}')
}

pub fn is_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The longest suffix of `text` made of characters that satisfy `accept` (a `[...]*$` match).
pub fn suffix_where(text: &str, accept: impl Fn(char) -> bool) -> &str {
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, ch)| accept(*ch))
        .last()
        .map_or(text.len(), |(index, _)| index);
    &text[start..]
}

/// `enum:<Table>` in a key's value type.
pub fn enum_table(value_type: &str) -> Option<&str> {
    let mut rest = value_type;
    while let Some(at) = rest.find("enum:") {
        rest = &rest[at + 5..];
        let word = &rest[..rest.find(|c| !is_word(c)).unwrap_or(rest.len())];
        if !word.is_empty() {
            return Some(word);
        }
    }
    None
}

/// A comment without its `;` or `;;` marker and the one space after it (`/^;;?\s?/`).
pub fn drop_marker(comment: &str) -> &str {
    let rest = comment.strip_prefix(';').unwrap_or(comment);
    let rest = rest.strip_prefix(';').unwrap_or(rest);
    let mut chars = rest.chars();
    match chars.next() {
        Some(ch) if ch.is_whitespace() => chars.as_str(),
        _ => rest,
    }
}
