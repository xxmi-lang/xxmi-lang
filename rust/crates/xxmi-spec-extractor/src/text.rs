//! JavaScript string semantics the extractor's output depends on:
//! which characters count as whitespace, and how many UTF-16 units a span has.

/// `\s` in JavaScript: ASCII whitespace, NBSP, the BOM and the Unicode space separators.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `\w` in JavaScript.
pub fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The longest prefix of `s` that is at most `units` UTF-16 units long (`slice(0, units)`).
pub fn prefix_units(s: &str, units: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        used += c.len_utf16();
        if used > units {
            return &s[..i];
        }
    }
    s
}

/// `Number(text)` when the result is an integer; `None` for NaN and fractions.
pub fn integer_value(text: &str) -> Option<i64> {
    let t = trim(text);
    if t.is_empty() {
        return Some(0);
    }

    let radix = |digits: &str, radix: u32| i64::from_str_radix(digits, radix).ok();
    let lower = t.to_ascii_lowercase();

    if let Some(d) = lower.strip_prefix("0x") {
        return radix(d, 16);
    }
    if let Some(d) = lower.strip_prefix("0o") {
        return radix(d, 8);
    }
    if let Some(d) = lower.strip_prefix("0b") {
        return radix(d, 2);
    }

    if lower.chars().any(|c| c.is_ascii_alphabetic() && c != 'e') {
        return None;
    }

    let value: f64 = lower.parse().ok()?;
    (value.is_finite() && value.fract() == 0.0 && value.abs() < 9e15).then_some(value as i64)
}
