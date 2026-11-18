//! Splits an ini into preamble, sections and lines the way the DLL's `ParseIniStream` does
//! (port of packages/core/src/model/scan.ts): lines are trimmed of spaces and tabs, blank lines are
//! skipped, a `;` as the first character makes a comment, any line starting with `[` is a section
//! header, and key/value lines split at the first `=`.
//!
//! Spans are UTF-8 byte offsets, so every cut falls on an ASCII byte and stays on a char boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn slice(self, text: &str) -> &str {
        &text[self.start..self.end]
    }
}

#[derive(Debug, Clone)]
pub struct ScannedLine {
    /// Zero-based.
    pub line: u32,
    /// The trimmed line.
    pub content: Span,
    /// Present when the line contains `=`: the trimmed text before it.
    pub key: Option<Span>,
    /// Present when the line contains `=`: the trimmed text after it (may be empty).
    pub value: Option<Span>,
}

#[derive(Debug)]
pub struct ScannedSection {
    /// Name between `[` and the first `]` (or end of line), trimmed.
    pub name: Span,
    pub header: ScannedLine,
    pub lines: Vec<ScannedLine>,
    /// From the header's first character to the start of the next header (or end of file).
    pub chunk: Span,
}

#[derive(Debug)]
pub struct ScannedIni {
    pub preamble: Vec<ScannedLine>,
    /// From the start of the file to the first section header (or end of file).
    pub preamble_chunk: Span,
    pub sections: Vec<ScannedSection>,
}

pub fn scan_ini(text: &str) -> ScannedIni {
    let bytes = text.as_bytes();
    let mut preamble = Vec::new();
    let mut sections: Vec<ScannedSection> = Vec::new();
    let mut first_header = text.len();

    let mut line_start = 0;
    let mut line_no = 0;
    loop {
        let line_end = find_byte(bytes, b'\n', line_start, text.len()).unwrap_or(text.len());
        let content = trimmed_line(bytes, line_start, line_end);

        if let Some(content) = content {
            let mut line = ScannedLine {
                line: line_no,
                content,
                key: None,
                value: None,
            };
            match bytes[content.start] {
                b';' => {}
                b'[' => {
                    if let Some(last) = sections.last_mut() {
                        last.chunk.end = line_start;
                    } else {
                        first_header = line_start;
                    }
                    let name = header_name(bytes, content);
                    sections.push(ScannedSection {
                        name,
                        header: line,
                        lines: Vec::new(),
                        chunk: Span {
                            start: line_start,
                            end: text.len(),
                        },
                    });
                }
                _ => {
                    split_key_value(bytes, &mut line);
                    match sections.last_mut() {
                        Some(section) => section.lines.push(line),
                        None => preamble.push(line),
                    }
                }
            }
        }

        if line_end >= text.len() {
            break;
        }
        line_start = line_end + 1;
        line_no += 1;
    }

    ScannedIni {
        preamble,
        preamble_chunk: Span {
            start: 0,
            end: first_header,
        },
        sections,
    }
}

fn find_byte(bytes: &[u8], needle: u8, from: usize, to: usize) -> Option<usize> {
    bytes[from..to]
        .iter()
        .position(|&b| b == needle)
        .map(|i| from + i)
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// The line without its line break and surrounding spaces and tabs; `None` when it is blank.
fn trimmed_line(bytes: &[u8], line_start: usize, line_end: usize) -> Option<Span> {
    let mut end = line_end;
    if end > line_start && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    let span = trim_blanks(
        bytes,
        Span {
            start: line_start,
            end,
        },
    );
    (span.start < span.end).then_some(span)
}

fn trim_blanks(bytes: &[u8], span: Span) -> Span {
    let (mut start, mut end) = (span.start, span.end);
    while start < end && is_blank(bytes[start]) {
        start += 1;
    }
    while end > start && is_blank(bytes[end - 1]) {
        end -= 1;
    }
    Span { start, end }
}

fn header_name(bytes: &[u8], header: Span) -> Span {
    let name_end = find_byte(bytes, b']', header.start, header.end).unwrap_or(header.end);
    trim_blanks(
        bytes,
        Span {
            start: header.start + 1,
            end: name_end,
        },
    )
}

fn split_key_value(bytes: &[u8], line: &mut ScannedLine) {
    let Span { start, end } = line.content;
    let Some(eq) = find_byte(bytes, b'=', start, end) else {
        return;
    };
    line.key = Some(trim_blanks(bytes, Span { start, end: eq }));
    line.value = Some(trim_blanks(bytes, Span { start: eq + 1, end }));
}
