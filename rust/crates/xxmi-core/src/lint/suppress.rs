//! Inline suppression, written as plain `;` comments the DLL ignores (docs/02-diagnostics.md):
//!
//!   ; xxmi-disable-next-line XM204        next line with content only
//!   ; xxmi-disable XM401 ... ; xxmi-enable XM401
//!   ; xxmi-disable                         all rules, until `; xxmi-enable`

use std::collections::{HashMap, HashSet};

use crate::model::IniFile;

use super::is_rule_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    DisableNextLine,
    Disable,
    Enable,
}

/// Rule ids a region covers, or every rule.
type Ids = Option<HashSet<String>>;

struct Region {
    ids: Ids,
    from_line: u32,
    to_line: u32,
}

pub struct Suppressions {
    regions: Vec<Region>,
}

impl Suppressions {
    pub fn new(file: &IniFile) -> Self {
        let content_lines = content_lines(file);
        let last_line = file.text.split('\n').count() as u32;
        let mut regions = Vec::new();
        // Rule id (or `*` for all) to the line its `disable` started at.
        let mut open: HashMap<String, u32> = HashMap::new();

        for comment in &file.comments {
            let Some((verb, ids)) = parse_directive(file.slice(comment.content)) else {
                continue;
            };
            let line = comment.line;

            match verb {
                Verb::DisableNextLine => {
                    let next = next_content_line(&content_lines, line, last_line);
                    regions.push(Region {
                        ids: ids_of(&ids),
                        from_line: next,
                        to_line: next,
                    });
                }
                Verb::Disable => {
                    for id in keys_of(&ids) {
                        open.entry(id).or_insert(line);
                    }
                }
                Verb::Enable => {
                    let targets = if ids.is_empty() {
                        open.keys().cloned().collect()
                    } else {
                        ids
                    };
                    for id in targets {
                        if let Some(from_line) = open.remove(&id) {
                            regions.push(region_of(&id, from_line, line));
                        }
                    }
                }
            }
        }

        for (id, from_line) in open {
            regions.push(region_of(&id, from_line, u32::MAX));
        }
        Self { regions }
    }

    pub fn is_suppressed(&self, line: u32, id: &str) -> bool {
        self.regions.iter().any(|r| {
            line >= r.from_line
                && line <= r.to_line
                && r.ids.as_ref().is_none_or(|ids| ids.contains(id))
        })
    }
}

fn ids_of(ids: &[String]) -> Ids {
    (!ids.is_empty()).then(|| ids.iter().cloned().collect())
}

fn keys_of(ids: &[String]) -> Vec<String> {
    if ids.is_empty() {
        vec!["*".to_string()]
    } else {
        ids.to_vec()
    }
}

fn region_of(id: &str, from_line: u32, to_line: u32) -> Region {
    Region {
        ids: (id != "*").then(|| HashSet::from([id.to_string()])),
        from_line,
        to_line,
    }
}

/// Lines a `disable-next-line` can land on: preamble, section headers and section lines (not
/// comments).
fn content_lines(file: &IniFile) -> HashSet<u32> {
    let mut lines: HashSet<u32> = file.preamble.iter().map(|l| l.scanned.line).collect();
    for section in &file.sections {
        lines.insert(section.header.line);
        lines.extend(section.lines.iter().map(|l| l.scanned.line));
    }
    lines
}

fn next_content_line(content_lines: &HashSet<u32>, after: u32, last_line: u32) -> u32 {
    let mut line = after + 1;
    while line <= last_line && !content_lines.contains(&line) {
        line += 1;
    }
    line
}

/// `^;\s*xxmi-(disable-next-line|disable|enable)\b\s*(.*)$`, case-insensitively, with the rule
/// ids of the tail (`XMnnn`, split on whitespace and commas, upper-cased).
fn parse_directive(comment: &str) -> Option<(Verb, Vec<String>)> {
    let rest = comment.strip_prefix(';')?.trim_start();
    let rest = strip_prefix_caseless(rest, "xxmi-")?;

    let verbs = [
        ("disable-next-line", Verb::DisableNextLine),
        ("disable", Verb::Disable),
        ("enable", Verb::Enable),
    ];
    for (name, verb) in verbs {
        let Some(tail) = strip_prefix_caseless(rest, name) else {
            continue;
        };
        if tail.chars().next().is_some_and(is_word_char) {
            continue;
        }
        return Some((verb, parse_ids(tail)));
    }
    None
}

fn parse_ids(tail: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for word in tail.split(|c: char| c.is_whitespace() || c == ',') {
        let id = word.to_uppercase();
        if is_rule_id(&id) && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn strip_prefix_caseless<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directives_parse_with_ids() {
        let (verb, ids) = parse_directive("; xxmi-disable-next-line xm204, XM201 nope").unwrap();
        assert_eq!(verb, Verb::DisableNextLine);
        assert_eq!(ids, ["XM204", "XM201"]);
    }

    #[test]
    fn verbs_need_a_word_boundary() {
        assert!(parse_directive("; xxmi-enabled").is_none());
        assert_eq!(parse_directive("; xxmi-enable").unwrap().0, Verb::Enable);
        assert_eq!(parse_directive(";xxmi-DISABLE").unwrap().0, Verb::Disable);
        assert!(parse_directive("; other").is_none());
    }
}
