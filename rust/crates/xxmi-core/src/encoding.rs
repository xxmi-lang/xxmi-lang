//! File bytes to text (port of `decodeText` in packages/core/src/parser/index.ts).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    /// The bytes aren't valid UTF-8 (likely GBK or Shift-JIS) and were decoded lossily.
    Unknown,
}

/// Decodes file bytes; a UTF-8 BOM is stripped so offsets match what editors show.
pub fn decode_text(bytes: &[u8]) -> (String, Encoding) {
    let (body, has_bom) = match bytes.strip_prefix(b"\xEF\xBB\xBF") {
        Some(rest) => (rest, true),
        None => (bytes, false),
    };

    match std::str::from_utf8(body) {
        Ok(text) => (
            text.to_string(),
            if has_bom {
                Encoding::Utf8Bom
            } else {
                Encoding::Utf8
            },
        ),
        Err(_) => (
            String::from_utf8_lossy(body).into_owned(),
            Encoding::Unknown,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_bom() {
        let (text, encoding) = decode_text(b"\xEF\xBB\xBF[A]");
        assert_eq!((text.as_str(), encoding), ("[A]", Encoding::Utf8Bom));
    }

    #[test]
    fn flags_invalid_utf8() {
        let (text, encoding) = decode_text(b"a\xFFb");
        assert_eq!((text.as_str(), encoding), ("a\u{FFFD}b", Encoding::Unknown));
    }
}
