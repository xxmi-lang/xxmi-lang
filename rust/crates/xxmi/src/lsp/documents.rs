//! The text of the open documents, kept in sync by incremental changes.

use line_index::LineIndex;
use xxmi_core::diagnostic::Range;
use xxmi_core::document::offset_at;

pub struct OpenDocument {
    pub uri: String,
    pub text: String,
}

/// A change event: a replaced range, or the whole text when `range` is `None`.
pub struct Change {
    pub range: Option<Range>,
    pub text: String,
}

/// Open documents in the order they were opened, which is the order diagnostics are published in.
#[derive(Default)]
pub struct Documents {
    open: Vec<OpenDocument>,
}

impl Documents {
    pub fn all(&self) -> &[OpenDocument] {
        &self.open
    }

    pub fn get(&self, uri: &str) -> Option<&OpenDocument> {
        self.open.iter().find(|d| d.uri == uri)
    }

    /// Reopening a document replaces its text and keeps its place.
    pub fn open(&mut self, uri: String, text: String) {
        match self.open.iter_mut().find(|d| d.uri == uri) {
            Some(document) => document.text = text,
            None => self.open.push(OpenDocument { uri, text }),
        }
    }

    pub fn close(&mut self, uri: &str) -> bool {
        let before = self.open.len();
        self.open.retain(|d| d.uri != uri);
        self.open.len() != before
    }

    /// Returns false for a document that isn't open.
    pub fn change(&mut self, uri: &str, changes: Vec<Change>) -> bool {
        let Some(document) = self.open.iter_mut().find(|d| d.uri == uri) else {
            return false;
        };
        for change in changes {
            apply(&mut document.text, change);
        }
        true
    }
}

fn apply(text: &mut String, change: Change) {
    let Some(range) = change.range else {
        *text = change.text;
        return;
    };

    let index = LineIndex::new(text);
    let start = offset_at(text, &index, range.start);
    let end = offset_at(text, &index, range.end).max(start);
    text.replace_range(start..end, &change.text);
}

#[cfg(test)]
mod tests {
    use xxmi_core::diagnostic::Position;

    use super::*;

    fn range(line: u32, from: u32, to: u32) -> Option<Range> {
        let at = |character| Position { line, character };
        Some(Range {
            start: at(from),
            end: at(to),
        })
    }

    #[test]
    fn applies_changes_in_order_with_utf16_columns() {
        let mut documents = Documents::default();
        documents.open("file:///a.ini".into(), "[A]\nk = 😀x\n".into());

        let changes = vec![
            Change {
                range: range(1, 6, 7),
                text: "y".into(),
            },
            Change {
                range: None,
                text: "whole".into(),
            },
            Change {
                range: range(0, 5, 99),
                text: "!".into(),
            },
        ];
        assert!(documents.change("file:///a.ini", changes));

        assert_eq!(documents.get("file:///a.ini").unwrap().text, "whole!");
    }

    #[test]
    fn ignores_changes_to_unopened_documents() {
        assert!(!Documents::default().change("file:///a.ini", Vec::new()));
    }
}
