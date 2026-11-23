//! Read-only lookups behind `xxmi query`.

pub mod docs;
pub mod library;
pub mod render;
pub mod spec;
pub mod spec_all;

use std::cmp::Ordering;

/// `a < b` on UTF-16 code units, which is how JavaScript sorts strings.
pub fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
