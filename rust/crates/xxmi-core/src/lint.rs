//! Runs the rules over a document and converts their byte spans to LSP ranges.

use line_index::{LineIndex, TextSize, WideEncoding};

use crate::diagnostic::{Diagnostic, Fix, Position, Range, TextEdit};
use crate::document::Document;
use crate::lookup::lookup;
use crate::model::lower;
use crate::rules::{Finding, FixSpec, RULES, Rule, RuleContext};
use crate::scan::Span;

/// Lints `doc` with every rule, or only with the rules in `only` (upper-case ids) when given.
pub fn lint(doc: &Document, only: Option<&[String]>) -> Vec<Diagnostic> {
    let model = lower(doc);
    let context = RuleContext {
        text: doc.text(),
        model: &model,
        lookup: lookup(),
    };
    let enabled = |rule: &&Rule| only.is_none_or(|ids| ids.iter().any(|id| id == rule.id));

    let mut diagnostics: Vec<Diagnostic> = RULES
        .iter()
        .filter(enabled)
        .flat_map(|rule| {
            (rule.check)(&context)
                .into_iter()
                .map(|finding| to_diagnostic(doc, rule, finding))
        })
        .collect();

    diagnostics.sort_by(|a, b| (a.range.start, a.id).cmp(&(b.range.start, b.id)));
    diagnostics
}

fn to_diagnostic(doc: &Document, rule: &Rule, finding: Finding) -> Diagnostic {
    Diagnostic {
        id: rule.id,
        severity: rule.severity,
        message: finding.message,
        range: range_of(doc.line_index(), finding.span),
        fix: finding.fix.map(|fix| to_fix(doc, fix)),
    }
}

fn to_fix(doc: &Document, fix: FixSpec) -> Fix {
    let edit = TextEdit {
        range: range_of(doc.line_index(), fix.span),
        new_text: fix.new_text,
    };
    Fix {
        title: fix.title,
        edits: vec![edit],
    }
}

fn range_of(index: &LineIndex, span: Span) -> Range {
    Range {
        start: position_of(index, span.start),
        end: position_of(index, span.end),
    }
}

/// Byte offset to a line and UTF-16 column.
fn position_of(index: &LineIndex, offset: usize) -> Position {
    let line_col = index.line_col(TextSize::new(offset as u32));
    let wide = index
        .to_wide(WideEncoding::Utf16, line_col)
        .expect("offsets sit on char boundaries");
    Position {
        line: wide.line,
        character: wide.col,
    }
}
