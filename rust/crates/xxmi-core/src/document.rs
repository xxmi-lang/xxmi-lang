//! An open document: its text, line index, line scan and one tree per section.
//!
//! Sections are parsed on their own so a syntax error can't spill into the next section.
//! An edit re-scans the text, then re-parses only the sections whose text changed,
//! handing tree-sitter the previous tree of the same section as the incremental base.

use std::cell::OnceCell;
use std::mem;

use line_index::{LineIndex, WideEncoding, WideLineCol};
use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::diagnostic::{Position, Range};
use crate::model::ChunkAnalysis;
use crate::parse::new_parser;
use crate::scan::{ScannedIni, Span, scan_ini};

/// One LSP content change: a range replacement, or the whole text when `range` is `None`.
#[derive(Debug, Clone)]
pub struct TextChange {
    pub range: Option<Range>,
    pub text: String,
}

/// A parsed slice of the file: the preamble, or one section from its header to the next header.
pub struct Chunk {
    pub span: Span,
    pub tree: Tree,
    /// Filled by lowering; dropped with the chunk when its text changes.
    pub analysis: OnceCell<ChunkAnalysis>,
}

impl Chunk {
    fn new(span: Span, tree: Tree) -> Self {
        Self {
            span,
            tree,
            analysis: OnceCell::new(),
        }
    }
}

pub struct Document {
    text: String,
    line_index: LineIndex,
    scan: ScannedIni,
    chunks: Vec<Chunk>,
    has_preamble: bool,
    parser: Parser,
}

impl Document {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut parser = new_parser();
        let scan = scan_ini(&text);
        let (spans, has_preamble) = chunk_spans(&text, &scan);
        let chunks = spans
            .iter()
            .map(|&span| fresh_chunk(&mut parser, &text, span))
            .collect();

        Self {
            line_index: LineIndex::new(&text),
            text,
            scan,
            chunks,
            has_preamble,
            parser,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    pub fn scan(&self) -> &ScannedIni {
        &self.scan
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// The preamble's chunk, when the text before the first header has anything but whitespace.
    pub fn preamble_chunk(&self) -> Option<&Chunk> {
        self.has_preamble.then(|| &self.chunks[0])
    }

    pub fn section_chunk(&self, index: usize) -> &Chunk {
        &self.chunks[index + usize::from(self.has_preamble)]
    }

    /// Replaces the text; only the sections that differ from the old text are parsed again.
    pub fn set_text(&mut self, text: String) {
        let index = LineIndex::new(&text);
        self.reparse(text, index);
    }

    /// Applies the changes in order, then re-parses what they touched.
    pub fn apply_changes(&mut self, changes: &[TextChange]) {
        let mut text = self.text.clone();
        let mut index = self.line_index.clone();

        for change in changes {
            match change.range {
                Some(range) => {
                    let start = offset_at(&text, &index, range.start);
                    let end = offset_at(&text, &index, range.end).max(start);
                    text.replace_range(start..end, &change.text);
                }
                None => text.clone_from(&change.text),
            }
            index = LineIndex::new(&text);
        }

        self.reparse(text, index);
    }

    fn reparse(&mut self, text: String, line_index: LineIndex) {
        let old_text = mem::replace(&mut self.text, text);
        let old_chunks = mem::take(&mut self.chunks);

        self.line_index = line_index;
        self.scan = scan_ini(&self.text);
        let (spans, has_preamble) = chunk_spans(&self.text, &self.scan);
        self.has_preamble = has_preamble;
        self.chunks = reparse_chunks(&mut self.parser, &old_text, old_chunks, &self.text, &spans);
    }
}

/// The spans to parse: the preamble when it holds anything, then each section.
fn chunk_spans(text: &str, scan: &ScannedIni) -> (Vec<Span>, bool) {
    let has_preamble = scan
        .preamble_chunk
        .slice(text)
        .chars()
        .any(|c| !c.is_whitespace());
    let preamble = has_preamble.then_some(scan.preamble_chunk);
    let spans = preamble
        .into_iter()
        .chain(scan.sections.iter().map(|s| s.chunk))
        .collect();
    (spans, has_preamble)
}

fn fresh_chunk(parser: &mut Parser, text: &str, span: Span) -> Chunk {
    let tree = parser
        .parse(span.slice(text), None)
        .expect("the parser has a language and no timeout");
    Chunk::new(span, tree)
}

/// LSP position to a byte offset, clamped to the line like the spec asks for characters past its end.
pub fn offset_at(text: &str, index: &LineIndex, position: Position) -> usize {
    let Some(line) = index.line(position.line) else {
        return text.len();
    };
    let start = usize::from(line.start());
    let end = usize::from(line.end());
    let content_end = if text[start..end].ends_with('\n') {
        end - 1
    } else {
        end
    };

    let wide = WideLineCol {
        line: position.line,
        col: position.character,
    };
    let col = index
        .to_utf8(WideEncoding::Utf16, wide)
        .map_or(position.character, |lc| lc.col);

    let mut offset = (start + col as usize).min(content_end);
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Chunks whose text is unchanged keep their tree;
/// the others re-parse from the tree of the chunk at the same place in the old list.
fn reparse_chunks(
    parser: &mut Parser,
    old_text: &str,
    old_chunks: Vec<Chunk>,
    text: &str,
    spans: &[Span],
) -> Vec<Chunk> {
    let old_spans: Vec<Span> = old_chunks.iter().map(|c| c.span).collect();
    let alignment = Alignment::new(old_text, &old_spans, text, spans);
    let mut old_chunks: Vec<Option<Chunk>> = old_chunks.into_iter().map(Some).collect();

    spans
        .iter()
        .enumerate()
        .map(|(new_index, &span)| {
            let old = alignment
                .old_index(new_index)
                .and_then(|i| old_chunks[i].take());
            match old {
                Some(old) => reparse_chunk(parser, old_text, old, text, span),
                None => fresh_chunk(parser, text, span),
            }
        })
        .collect()
}

fn reparse_chunk(parser: &mut Parser, old_text: &str, old: Chunk, text: &str, span: Span) -> Chunk {
    let (old_source, source) = (old.span.slice(old_text), span.slice(text));
    if old_source == source {
        return Chunk {
            span,
            tree: old.tree,
            analysis: old.analysis,
        };
    }

    let mut base = old.tree;
    base.edit(&diff_edit(old_source, source));
    let tree = parser
        .parse(source, Some(&base))
        .expect("the parser has a language and no timeout");
    Chunk::new(span, tree)
}

/// Pairs new chunks with old ones: the identical leading and trailing chunks,
/// then the changed middle ones in order.
struct Alignment {
    prefix: usize,
    suffix: usize,
    old_len: usize,
    new_len: usize,
}

impl Alignment {
    fn new(old_text: &str, old: &[Span], text: &str, new: &[Span]) -> Self {
        let same = |(o, n): &(&Span, &Span)| o.slice(old_text) == n.slice(text);
        let prefix = old.iter().zip(new).take_while(same).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(same)
            .count();
        Self {
            prefix,
            suffix,
            old_len: old.len(),
            new_len: new.len(),
        }
    }

    fn old_index(&self, new_index: usize) -> Option<usize> {
        if new_index < self.prefix {
            return Some(new_index);
        }
        if new_index >= self.new_len - self.suffix {
            return Some(self.old_len - (self.new_len - new_index));
        }
        let candidate = new_index;
        (candidate < self.old_len - self.suffix).then_some(candidate)
    }
}

/// One tree-sitter edit covering what differs between two texts:
/// their common prefix and suffix stay out of it.
fn diff_edit(old: &str, new: &str) -> InputEdit {
    let prefix = common_prefix(old, new);
    let suffix = common_suffix(old, new, prefix);
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);

    InputEdit {
        start_byte: prefix,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point_at(old, prefix),
        old_end_position: point_at(old, old_end),
        new_end_position: point_at(new, new_end),
    }
}

fn common_prefix(a: &str, b: &str) -> usize {
    let equal = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    (0..=equal)
        .rev()
        .find(|&i| a.is_char_boundary(i) && b.is_char_boundary(i))
        .unwrap_or(0)
}

fn common_suffix(a: &str, b: &str, prefix: usize) -> usize {
    let room = a.len().min(b.len()) - prefix;
    let equal = a
        .bytes()
        .rev()
        .zip(b.bytes().rev())
        .take(room)
        .take_while(|(x, y)| x == y)
        .count();
    (0..=equal)
        .rev()
        .find(|&k| a.is_char_boundary(a.len() - k) && b.is_char_boundary(b.len() - k))
        .unwrap_or(0)
}

fn point_at(text: &str, offset: usize) -> Point {
    let before = &text[..offset];
    let row = before.bytes().filter(|&b| b == b'\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    Point {
        row,
        column: offset - line_start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn replace(start: Position, end: Position, text: &str) -> TextChange {
        TextChange {
            range: Some(Range { start, end }),
            text: text.to_string(),
        }
    }

    fn trees(doc: &Document) -> Vec<String> {
        doc.chunks()
            .iter()
            .map(|c| c.tree.root_node().to_sexp())
            .collect()
    }

    /// An incremental edit must leave the document as a fresh parse of the same text would.
    fn assert_edit(before: &str, change: TextChange, after: &str) {
        let mut doc = Document::new(before);
        doc.apply_changes(&[change]);

        assert_eq!(doc.text(), after);
        assert_eq!(trees(&doc), trees(&Document::new(after)));
    }

    #[test]
    fn single_character_insert() {
        let change = replace(at(1, 3), at(1, 3), "x");
        assert_edit("[A]\nkey = 1\n", change, "[A]\nkeyx = 1\n");
    }

    #[test]
    fn utf16_columns_count_astral_characters_as_two() {
        let change = replace(at(1, 4), at(1, 5), "Z");
        assert_edit(
            "[A]\n; \u{1F600}x\nkey = 1\n",
            change,
            "[A]\n; \u{1F600}Z\nkey = 1\n",
        );
    }

    #[test]
    fn utf16_columns_count_bmp_characters_as_one() {
        let change = replace(at(1, 3), at(1, 4), "!");
        assert_edit("[A]\n; \u{e9}x\n", change, "[A]\n; \u{e9}!\n");
    }

    #[test]
    fn multi_line_replace_joins_and_adds_sections() {
        let before = "[A]\nkey = 1\n[B]\nname = 2\n[C]\nz = 3\n";
        let change = replace(at(1, 3), at(4, 0), "\n[X]\nq = 9\n");
        assert_edit(before, change, "[A]\nkey\n[X]\nq = 9\n[C]\nz = 3\n");
    }

    #[test]
    fn crlf_lines_keep_their_carriage_return() {
        let before = "[A]\r\nkey = 1\r\n[B]\r\nz = 2\r\n";
        let change = replace(at(1, 6), at(2, 0), "7\r\n");
        assert_edit(before, change, "[A]\r\nkey = 7\r\n[B]\r\nz = 2\r\n");
    }

    #[test]
    fn positions_past_the_line_end_clamp_to_it() {
        let change = replace(at(0, 99), at(0, 99), "!");
        assert_edit("[A]\nk = 1\n", change, "[A]!\nk = 1\n");
    }

    #[test]
    fn positions_past_the_last_line_append() {
        let change = replace(at(9, 0), at(9, 0), "[B]\n");
        assert_edit("[A]\n", change, "[A]\n[B]\n");
    }

    #[test]
    fn whole_document_change_replaces_everything() {
        let change = TextChange {
            range: None,
            text: "[N]\nk = 1\n".to_string(),
        };
        assert_edit("[A]\nold = 1\n", change, "[N]\nk = 1\n");
    }

    #[test]
    fn preamble_appearing_shifts_the_chunks() {
        let change = replace(at(0, 0), at(0, 0), "namespace = M\n");
        assert_edit("[A]\nk = 1\n", change, "namespace = M\n[A]\nk = 1\n");
    }

    #[test]
    fn consecutive_changes_see_each_other() {
        let mut doc = Document::new("[A]\nk = 1\n");
        doc.apply_changes(&[
            replace(at(0, 3), at(0, 3), "\n[B]"),
            replace(at(1, 3), at(1, 3), "x"),
        ]);
        assert_eq!(doc.text(), "[A]\n[B]x\nk = 1\n");
    }
}
