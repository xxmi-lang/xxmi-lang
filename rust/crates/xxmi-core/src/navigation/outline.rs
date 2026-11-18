//! The document outline: sections grouped by kind.

use crate::diagnostic::Range;
use crate::model::IniFile;

pub struct OutlineSymbol {
    pub name: String,
    /// The spec section name (`Unknown` for an unknown prefix), or `group` for the grouping node.
    pub kind: &'static str,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<OutlineSymbol>,
}

/// Sections grouped by kind (docs/03-lsp.md `documentSymbol`), groups in first-seen order.
pub fn outline(file: &IniFile) -> Vec<OutlineSymbol> {
    let mut groups: Vec<(&'static str, OutlineSymbol)> = Vec::new();

    for section in &file.sections {
        let group = section.spec.map_or("Unknown", |s| s.name.as_str());
        let last = section.lines.last().map_or(&section.header, |l| &l.scanned);
        let range = Range {
            start: file.range_of(section.header.content).start,
            end: file.range_of(last.content).end,
        };
        let selection_range = file.range_of(section.name);
        let symbol = OutlineSymbol {
            name: file.slice(section.name).to_string(),
            kind: group,
            range,
            selection_range,
            children: Vec::new(),
        };

        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, existing)) => {
                existing.range.end = range.end;
                existing.children.push(symbol);
            }
            None => groups.push((
                group,
                OutlineSymbol {
                    name: group.to_string(),
                    kind: "group",
                    range,
                    selection_range,
                    children: vec![symbol],
                },
            )),
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}
