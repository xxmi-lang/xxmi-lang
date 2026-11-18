//! Doc comments of library symbols (docs/01-language-model.md "Doc comments"): the contiguous
//! `;` block directly above a section header or a `global` line, optionally with the structured
//! `;;` tags (`@desc`, `@in`, `@out`, `@since`, `@example`).

use serde::ser::{Serialize, SerializeMap, Serializer};

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocParam {
    pub name: String,
    pub description: String,
}

/// The `;;` tags. A JavaScript object lists `desc` and `since` in the order they were first
/// seen, then `example` last, so serialisation follows that.
#[derive(Debug, Clone, Default)]
pub struct DocTags {
    pub scalars: Vec<(&'static str, String)>,
    pub inputs: Vec<DocParam>,
    pub outputs: Vec<DocParam>,
    pub example: Option<String>,
}

impl DocTags {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.scalars
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    fn set(&mut self, name: &'static str, value: String) {
        match self.scalars.iter_mut().find(|(key, _)| *key == name) {
            Some(slot) => slot.1 = value,
            None => self.scalars.push((name, value)),
        }
    }
}

impl Serialize for DocTags {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("in", &self.inputs)?;
        map.serialize_entry("out", &self.outputs)?;
        for (key, value) in &self.scalars {
            map.serialize_entry(key, value)?;
        }
        if let Some(example) = &self.example {
            map.serialize_entry("example", example)?;
        }
        map.end()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocComment {
    /// 1-based line of the first comment line.
    pub line: usize,
    /// 1-based line of the last comment line.
    pub end_line: usize,
    /// The comment text with the `;` markers removed.
    pub text: String,
    /// Present when the block uses the structured `;;` convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<DocTags>,
}

struct Block<'a> {
    start: usize,
    lines: &'a [&'a str],
}

/// `text.split(/\r?\n/)`: unlike `str::lines`, a trailing newline yields a last empty line.
pub fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn is_comment(line: Option<&&str>) -> bool {
    line.is_some_and(|l| l.trim_start().starts_with(';'))
}

fn is_structured(line: &str) -> bool {
    line.trim_start().starts_with(";;")
}

fn strip_marker(line: &str) -> &str {
    let rest = line.trim_start().trim_start_matches(';');
    rest.strip_prefix([' ', '\t']).unwrap_or(rest)
}

/// Comment lines ending at 0-based `end` (inclusive), or `None` when line `end` isn't one.
fn block_ending_at<'a>(lines: &'a [&'a str], end: usize) -> Option<Block<'a>> {
    if !is_comment(lines.get(end)) {
        return None;
    }
    let mut start = end;
    while start > 0 && is_comment(lines.get(start - 1)) {
        start -= 1;
    }
    Some(Block {
        start,
        lines: &lines[start..=end],
    })
}

fn parse_param(rest: &str) -> DocParam {
    let rest = rest.trim();
    let split = rest.find(char::is_whitespace).unwrap_or(rest.len());
    DocParam {
        name: rest[..split].to_string(),
        description: rest[split..].trim().to_string(),
    }
}

/// `@name rest` on one line.
fn parse_tag(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim().strip_prefix('@')?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| (&rest[..end], rest[end..].trim_start()))
}

fn parse_tags(lines: &[&str]) -> DocTags {
    let mut tags = DocTags::default();
    let mut example: Vec<&str> = Vec::new();
    let mut in_example = false;

    for raw in lines {
        let text = strip_marker(raw);
        let Some((name, rest)) = parse_tag(text) else {
            if in_example {
                example.push(text);
            }
            continue;
        };

        in_example = name == "example";
        match name {
            "desc" => tags.set("desc", rest.to_string()),
            "since" => tags.set("since", rest.to_string()),
            "in" => tags.inputs.push(parse_param(rest)),
            "out" => tags.outputs.push(parse_param(rest)),
            _ => {}
        }
    }

    if !example.is_empty() {
        tags.example = Some(example.join("\n").trim_end().to_string());
    }
    tags
}

fn to_doc(block: &Block) -> DocComment {
    let text = block
        .lines
        .iter()
        .map(|l| strip_marker(l))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string();
    let line = block.start + 1;
    let end_line = block.start + block.lines.len();

    if !block.lines.iter().any(|l| is_structured(l)) {
        return DocComment {
            line,
            end_line,
            text,
            tags: None,
        };
    }

    let tags = parse_tags(block.lines);
    DocComment {
        line,
        end_line,
        text: tags.get("desc").map_or(text, str::to_string),
        tags: Some(tags),
    }
}

/// Doc comment ending directly above 0-based `line` (a section header or a declaration).
pub fn doc_above(file_lines: &[&str], line: usize) -> Option<DocComment> {
    let end = line.checked_sub(1)?;
    block_ending_at(file_lines, end).map(|block| to_doc(&block))
}

/// Every contiguous comment block of a file, in order.
pub fn comment_blocks(file_lines: &[&str]) -> Vec<DocComment> {
    let mut blocks = Vec::new();
    for i in 0..file_lines.len() {
        if !is_comment(file_lines.get(i)) || is_comment(file_lines.get(i + 1)) {
            continue;
        }
        if let Some(block) = block_ending_at(file_lines, i) {
            blocks.push(to_doc(&block));
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_block_above_a_line_is_its_doc() {
        let lines = split_lines("; one\n;two\n[Section]\n");
        let doc = doc_above(&lines, 2).expect("a doc");
        assert_eq!((doc.line, doc.end_line), (1, 2));
        assert_eq!(doc.text, "one\ntwo");
        assert!(doc.tags.is_none());
    }

    #[test]
    fn structured_block_keeps_tags_in_first_seen_order() {
        let lines = split_lines(
            ";; @since 1.2\n;; @desc Does it\n;; @in x The input\n;; @example\n;; run()\n[S]",
        );
        let doc = doc_above(&lines, 5).expect("a doc");
        let tags = doc.tags.expect("tags");
        assert_eq!(doc.text, "Does it");
        assert_eq!(tags.scalars[0], ("since", "1.2".to_string()));
        assert_eq!(tags.inputs[0].name, "x");
        assert_eq!(tags.example.as_deref(), Some("run()"));
    }

    #[test]
    fn a_blank_line_ends_the_block() {
        let lines = split_lines("; far\n\n[S]");
        assert!(doc_above(&lines, 2).is_none());
    }
}
