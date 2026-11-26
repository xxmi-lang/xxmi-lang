//! The quick fix that declares a global in the file's own [Constants].

use super::{FixSpec, RuleContext, insert_line_after, line_ending};
use crate::scan::Span;

/// Adds `global $name` to the first [Constants] section, or opens one near the top of the file.
/// The DLL namespaces a global by the file that declares it, so it has to be this file's.
pub fn declare_global(context: &RuleContext, name: &str) -> FixSpec {
    let declaration = format!("global {name}");
    let (span, new_text) = match constants_anchor(context) {
        Some((end, content_start)) => {
            insert_line_after(context.text, end, content_start, &declaration)
        }
        None => open_constants(context, &declaration),
    };

    FixSpec {
        title: format!("Declare {name} in [Constants]"),
        span,
        new_text,
        also: Vec::new(),
    }
}

/// Where the first [Constants] section ends, and where its last line's content starts.
fn constants_anchor(context: &RuleContext) -> Option<(usize, usize)> {
    let section = context.file.sections.iter().find(|section| {
        context
            .file
            .slice(section.name)
            .eq_ignore_ascii_case("constants")
    })?;
    let last = section.lines.last().map_or(&section.header, |l| &l.scanned);
    Some((last.content.end, last.content.start))
}

fn open_constants(context: &RuleContext, declaration: &str) -> (Span, String) {
    let text = context.text;
    let at = preamble_end(context);
    let eol = line_ending(text, at);
    // A key line straight above would otherwise run into the header.
    let gap = if text[..at]
        .lines()
        .next_back()
        .is_some_and(|line| !line.trim().is_empty())
    {
        eol
    } else {
        ""
    };
    (
        Span { start: at, end: at },
        format!("{gap}[Constants]{eol}{declaration}{eol}{eol}"),
    )
}

/// The start of the line after the last key or blank line before the first section.
/// Keys there (`namespace`, `condition`, in any order) belong to the file, not to a section.
/// A comment block touching the first section stays attached to it, so the new section goes above.
fn preamble_end(context: &RuleContext) -> usize {
    let limit = context
        .file
        .sections
        .first()
        .map_or(context.text.len(), |section| section.chunk.start);
    let mut anchor = 0;
    let mut end = 0;

    for line in context.text[..limit].split_inclusive('\n') {
        end += line.len();
        if !line.trim_start().starts_with(';') {
            anchor = end;
        }
    }
    anchor
}
