//! The model rules run on: scanned lines classified against the spec, plus what tree-sitter says
//! about each line (port of the parts of packages/core/src/model/lower.ts the two rules read).

use tree_sitter::Node;

use crate::document::{Chunk, Document};
use crate::lookup::lookup;
use crate::parse::collect_syntax_issues;
use crate::scan::{ScannedLine, Span};
use crate::spec::{SectionKind, SpecSection};

/// Nodes whose children the model does not descend into, except for these children.
const REFERENCE_NODES: [&str; 6] = [
    "callable_commandlist",
    "callable_customshader",
    "custom_resource",
    "resource_pool",
    "preset_section_identifier",
    "named_variable",
];
const REFERENCE_CHILDREN: [&str; 2] = ["index_expression", "arguments"];
const DECLARATIONS: [&str; 4] = [
    "global_declaration",
    "global_initialisation",
    "local_declaration",
    "local_initialisation",
];

/// A syntax issue with offsets into the whole file.
#[derive(Debug, Clone)]
pub struct Issue {
    pub span: Span,
    /// Zero-based line the issue starts on.
    pub line: u32,
    pub message: String,
}

#[derive(Debug)]
pub struct LineModel {
    pub scanned: ScannedLine,
    /// Lower-cased key with any `pre `/`post ` prefix removed (what the DLL dispatches on).
    pub command: Option<String>,
    /// Kind of the first statement node tree-sitter starts on this line.
    pub statement: Option<String>,
}

#[derive(Debug)]
pub struct SectionModel {
    pub name: Span,
    /// `None` for an unknown section prefix.
    pub spec: Option<&'static SpecSection>,
    pub lines: Vec<LineModel>,
    pub issues: Vec<Issue>,
}

#[derive(Debug)]
pub struct Model {
    pub preamble: Vec<LineModel>,
    pub preamble_issues: Vec<Issue>,
    pub sections: Vec<SectionModel>,
}

pub fn lower(doc: &Document) -> Model {
    let text = doc.text();
    let scan = doc.scan();

    let mut preamble = line_models(text, &scan.preamble, false);
    let mut preamble_issues = Vec::new();
    if let Some(chunk) = doc.preamble_chunk() {
        analyse_chunk(text, chunk, 0, &mut preamble, &mut preamble_issues);
    }

    let sections = scan
        .sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let spec = lookup().section_kind(section.name.slice(text));
            let is_command_list = spec.is_some_and(|s| s.kind == SectionKind::CommandList);

            let mut lines = line_models(text, &section.lines, is_command_list);
            let mut issues = Vec::new();
            let chunk = doc.section_chunk(index);
            analyse_chunk(text, chunk, section.header.line, &mut lines, &mut issues);

            SectionModel {
                name: section.name,
                spec,
                lines,
                issues,
            }
        })
        .collect();

    Model {
        preamble,
        preamble_issues,
        sections,
    }
}

fn line_models(text: &str, lines: &[ScannedLine], is_command_list: bool) -> Vec<LineModel> {
    lines
        .iter()
        .map(|line| LineModel {
            scanned: line.clone(),
            command: line
                .key
                .map(|key| command_of(key.slice(text), is_command_list)),
            statement: None,
        })
        .collect()
}

/// What the DLL dispatches on: the lower-cased key, minus the `pre `/`post ` list selector of a
/// command-list line.
fn command_of(key: &str, is_command_list: bool) -> String {
    let lower = key.to_lowercase();
    if !is_command_list {
        return lower;
    }

    for modifier in ["pre", "post"] {
        if let Some(rest) = lower.strip_prefix(modifier)
            && rest.starts_with(char::is_whitespace)
        {
            return rest.trim_start().to_string();
        }
    }
    lower
}

/// Reads one chunk's tree: its syntax issues (shifted to file offsets) and the statement kind of
/// each line. `chunk_line` is the file line the chunk starts on.
fn analyse_chunk(
    text: &str,
    chunk: &Chunk,
    chunk_line: u32,
    lines: &mut [LineModel],
    issues: &mut Vec<Issue>,
) {
    let source = chunk.span.slice(text);
    for issue in collect_syntax_issues(&chunk.tree, source) {
        issues.push(Issue {
            span: Span {
                start: issue.span.start + chunk.span.start,
                end: issue.span.end + chunk.span.start,
            },
            line: chunk_line + issue.row as u32,
            message: issue.message,
        });
    }
    assign_statements(chunk.tree.root_node(), chunk_line, lines);
}

/// Records, per line, the first statement node that starts on it. Text inside an ERROR node is
/// reported as XM001 instead of being interpreted.
fn assign_statements(node: Node<'_>, chunk_line: u32, lines: &mut [LineModel]) {
    if node.is_error() {
        return;
    }

    let kind = node.kind();
    if is_statement(kind) {
        let row = chunk_line + node.start_position().row as u32;
        if let Ok(i) = lines.binary_search_by_key(&row, |l| l.scanned.line)
            && lines[i].statement.is_none()
        {
            lines[i].statement = Some(kind.to_string());
        }
    }

    if DECLARATIONS.contains(&kind) {
        let value = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("expression"));
        if let Some(value) = value {
            assign_statements(value, chunk_line, lines);
        }
        return;
    }

    let is_reference = REFERENCE_NODES.contains(&kind);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if !is_reference || REFERENCE_CHILDREN.contains(&child.kind()) {
            assign_statements(child, chunk_line, lines);
        }
    }
}

fn is_statement(kind: &str) -> bool {
    [
        "_statement",
        "_instruction",
        "_declaration",
        "_initialisation",
    ]
    .iter()
    .any(|suffix| kind.ends_with(suffix))
}
