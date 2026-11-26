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
        None => open_constants(context.text, &declaration),
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

fn open_constants(text: &str, declaration: &str) -> (Span, String) {
    let at = top_of_file(text);
    let eol = line_ending(text, at);
    (
        Span { start: at, end: at },
        format!("[Constants]{eol}{declaration}{eol}{eol}"),
    )
}

/// The start of the line after the `namespace` line or the last blank line at the top.
/// A comment block touching the first section stays attached to it, so the new section goes above.
fn top_of_file(text: &str) -> usize {
    let mut anchor = 0;
    let mut end = 0;

    for line in text.split_inclusive('\n') {
        end += line.len();
        let trimmed = line.trim();
        if trimmed.is_empty() || is_namespace(trimmed) {
            anchor = end;
        } else if !trimmed.starts_with(';') {
            break;
        }
    }
    anchor
}

fn is_namespace(line: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("namespace"))
}
