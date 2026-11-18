//! Read-only access to indexed checkouts: a source tree for the DLL scans, `read_source`
//! excerpts and ripgrep-backed `search_source`. Paths from callers are normalized and must stay
//! inside the checkout (docs/06-mcp-server.md "Security and limits").

pub mod excerpt;
pub mod implementation;
pub mod search;
pub mod tree;

use crate::catalog::ResolvedRef;

/// A failure the caller can act on: a bad path or pattern, a missing tool.
#[derive(Debug)]
pub struct SourceError(pub String);

/// JavaScript's `encodeURIComponent`: everything but letters, digits and `-_.!~*'()` is escaped.
fn encode_uri_component(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `https://github.com/<owner>/<repo>/blob/<sha>/<path>#L<line>`
pub fn permalink(reference: &ResolvedRef, path: &str, line: Option<usize>) -> String {
    let encoded: Vec<String> = path.split('/').map(encode_uri_component).collect();
    let anchor = line
        .filter(|n| *n > 0)
        .map_or(String::new(), |n| format!("#L{n}"));
    format!(
        "https://github.com/{}/blob/{}/{}{anchor}",
        reference.repo.full_name(),
        reference.sha,
        encoded.join("/")
    )
}

/// The first `max` UTF-16 code units, as JavaScript's `slice(0, max)` cuts a string.
pub fn slice_utf16(text: &str, max: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().take(max).collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_like_encode_uri_component() {
        assert_eq!(encode_uri_component("a b/é(1)"), "a%20b%2F%C3%A9(1)");
    }

    #[test]
    fn slices_by_utf16_units() {
        assert_eq!(slice_utf16("abcdef", 3), "abc");
        assert_eq!(slice_utf16("😀😀", 2), "😀");
    }
}
