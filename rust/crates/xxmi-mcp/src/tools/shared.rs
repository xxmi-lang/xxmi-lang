//! Plumbing shared by the tool handlers: context, output shape, arguments, pagination.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Map, Value};

use super::blocking::BlockingGate;
use crate::catalog::{Catalog, CatalogError};
use crate::config::Config;
use crate::logger::Logger;
use crate::packages::PackageCache;
use crate::sources::SourceError;
use crate::specs::{ResolvedSpec, SpecRegistry};

pub struct ToolContext {
    pub config: Config,
    pub catalog: Arc<Catalog>,
    pub logger: Logger,
    /// Every spec the server can answer with; tools pick one through `resolve_spec`.
    pub specs: SpecRegistry,
    pub packages: PackageCache,
    pub blocking: BlockingGate,
}

impl ToolContext {
    pub fn resolve_spec(&self, dll_ref: Option<&str>) -> Result<ResolvedSpec, ToolError> {
        Ok(self.specs.resolve(&self.catalog, dll_ref)?)
    }
}

/// What a handler returns: the compact text and the structured value behind it.
pub struct ToolOutput {
    pub text: String,
    pub data: Value,
}

/// `expected` failures are ones the caller can act on (bad input, unknown ref, warming up);
/// the rest are bugs, and their message stays in the log.
#[derive(Debug)]
pub struct ToolError {
    pub message: String,
    pub expected: bool,
    /// The server is saturated: the caller should retry, and gets a protocol error, not a result.
    pub busy: bool,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            expected: true,
            busy: false,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            expected: false,
            busy: false,
        }
    }

    pub fn busy() -> Self {
        Self {
            busy: true,
            ..Self::new("Server busy: too many tool calls in progress, retry shortly.")
        }
    }
}

impl From<CatalogError> for ToolError {
    fn from(error: CatalogError) -> Self {
        Self::new(error.0)
    }
}

impl From<SourceError> for ToolError {
    fn from(error: SourceError) -> Self {
        Self::new(error.0)
    }
}

pub type ToolResult = Result<ToolOutput, ToolError>;

pub fn to_data(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Validated arguments: the schema has already checked every type and bound.
pub struct Args(pub Map<String, Value>);

impl Args {
    pub fn str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    /// A string the schema marks required.
    pub fn required(&self, key: &str) -> &str {
        self.str(key).unwrap_or_default()
    }

    /// A non-empty string: JavaScript treats `""` as absent in the checks that use this.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.str(key).filter(|s| !s.is_empty())
    }

    pub fn count(&self, key: &str) -> Option<usize> {
        self.0.get(key).and_then(Value::as_u64).map(|n| n as usize)
    }

    pub fn wants_json(&self) -> bool {
        self.str("format") == Some("json")
    }
}

/// Adds the `spec` block to the structured result and the one-line summary to the text.
pub fn with_spec(output: ToolOutput, spec: &ResolvedSpec) -> ToolOutput {
    let mut data = output.data;
    if let Value::Object(map) = &mut data {
        map.insert("spec".into(), spec.block());
    }
    ToolOutput {
        text: format!("{}\n{}", output.text, spec.line()),
        data,
    }
}

/// The keys of `base` followed by those of `extra`: a JavaScript object spread.
pub fn spread(mut base: Map<String, Value>, extra: &impl Serialize) -> Value {
    if let Value::Object(more) = to_data(extra) {
        base.extend(more);
    }
    Value::Object(base)
}

pub const PAGE_SIZE: usize = 100;

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_cursor(offset: usize) -> String {
    let bytes = format!("o{offset}").into_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(char::from(BASE64URL[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// Lenient like Node's `Buffer.from(text, 'base64url')`:
/// characters outside the alphabet are skipped and decoding stops at `=`.
fn decode_cursor(cursor: &str) -> String {
    let mut bytes = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in cursor.chars().take_while(|c| *c != '=') {
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '-' | '+' => 62,
            '_' | '/' => 63,
            _ => continue,
        };
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((acc >> bits) & 0xff) as u8);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn cursor_offset(cursor: &str) -> Result<usize, ToolError> {
    let decoded = decode_cursor(cursor);
    let digits = decoded.strip_prefix('o').unwrap_or("");
    let valid = (1..=9).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit());
    match digits.parse::<usize>() {
        Ok(offset) if valid => Ok(offset),
        _ => Err(ToolError::new("invalid cursor")),
    }
}

pub fn page_of<T>(
    items: Vec<T>,
    cursor: Option<&str>,
) -> Result<(Vec<T>, Option<String>), ToolError> {
    let offset = match cursor.filter(|c| !c.is_empty()) {
        Some(cursor) => cursor_offset(cursor)?,
        None => 0,
    };
    let next = offset + PAGE_SIZE;
    let next_cursor = (next < items.len()).then(|| encode_cursor(next));
    let page = items.into_iter().skip(offset).take(PAGE_SIZE).collect();
    Ok((page, next_cursor))
}

pub fn next_cursor_line(next_cursor: Option<&str>) -> Vec<String> {
    next_cursor
        .map(|c| format!("next cursor: {c} (pass as \"cursor\" for more)"))
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_like_base64url() {
        assert_eq!(encode_cursor(100), "bzEwMA");
        assert_eq!(decode_cursor("bzEwMA"), "o100");
        assert_eq!(cursor_offset(&encode_cursor(2500)).unwrap(), 2500);
    }

    #[test]
    fn pages_by_one_hundred() {
        let items: Vec<usize> = (0..250).collect();
        let (first, next) = page_of(items.clone(), None).unwrap();
        assert_eq!((first.len(), first[0]), (100, 0));

        let (second, next) = page_of(items.clone(), next.as_deref()).unwrap();
        assert_eq!(second[0], 100);

        let (last, next) = page_of(items, next.as_deref()).unwrap();
        assert_eq!((last.len(), next), (50, None));
    }

    #[test]
    fn rejects_malformed_cursors() {
        for bad in ["!!", "bg", "b2FiYw"] {
            assert!(page_of(vec![1], Some(bad)).is_err(), "{bad}");
        }
    }
}
