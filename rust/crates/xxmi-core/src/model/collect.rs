//! Walks one chunk's tree for statements, references and variable declarations (the `collect`
//! half of packages/core/src/model/lower.ts).

use tree_sitter::Node;

use crate::document::Chunk;
use crate::scan::Span;

use super::lower::ChunkTarget;
use super::types::{Access, Reference, Scope, SymbolKind, VariableDeclaration};

/// Statements whose `name` field is written, not read.
const ASSIGNMENTS: [&str; 3] = [
    "assignment_statement",
    "key_assignment_statement",
    "preset_assignment_statement",
];

const DECLARATIONS: [&str; 4] = [
    "global_declaration",
    "global_initialisation",
    "local_declaration",
    "local_initialisation",
];

/// Reference nodes whose children can hold more references (pool indices, member arguments).
const REFERENCE_CHILDREN: [&str; 2] = ["index_expression", "arguments"];

/// Nodes whose text is a reference, and the kind they refer to.
fn reference_kind(node_kind: &str) -> Option<SymbolKind> {
    Some(match node_kind {
        "callable_commandlist" => SymbolKind::CommandList,
        "callable_customshader" => SymbolKind::CustomShader,
        "custom_resource" => SymbolKind::Resource,
        "resource_pool" => SymbolKind::Pool,
        "preset_section_identifier" => SymbolKind::Preset,
        "named_variable" => SymbolKind::Variable,
        _ => return None,
    })
}

pub(super) fn collect(
    chunk: &Chunk,
    source: &str,
    target: &mut ChunkTarget,
    references: &mut Vec<Reference>,
    variables: &mut Vec<VariableDeclaration>,
) {
    let mut walker = Walker {
        source,
        offset: chunk.span.start,
        target,
        references,
        variables,
    };
    walker.visit(chunk.tree.root_node());
}

struct Walker<'a, 'b> {
    source: &'a str,
    /// Trees are built from the chunk's text alone; this shifts them to file offsets.
    offset: usize,
    target: &'a mut ChunkTarget<'b>,
    references: &'a mut Vec<Reference>,
    variables: &'a mut Vec<VariableDeclaration>,
}

impl Walker<'_, '_> {
    fn visit(&mut self, node: Node) {
        // Text inside an ERROR is reported as XM001, not interpreted.
        if node.is_error() {
            return;
        }
        self.record_statement(node);

        if DECLARATIONS.contains(&node.kind()) {
            self.visit_declaration(node);
        } else if let Some(kind) = reference_kind(node.kind()) {
            self.visit_reference(node, kind);
        } else {
            self.visit_children(node, |_| true);
        }
    }

    fn visit_children(&mut self, node: Node, wanted: impl Fn(Node) -> bool) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if wanted(child) {
                self.visit(child);
            }
        }
    }

    fn record_statement(&mut self, node: Node) {
        let kind = node.kind();
        if !is_statement(kind) {
            return;
        }

        let row = self.target.line + node.start_position().row as u32;
        if let Ok(i) = self
            .target
            .lines
            .binary_search_by_key(&row, |l| l.scanned.line)
            && self.target.lines[i].statement.is_none()
        {
            self.target.lines[i].statement = Some(kind.to_string());
        }
    }

    fn visit_declaration(&mut self, node: Node) {
        if let Some(variable) = node.child_by_field_name("variable") {
            let has_flag = |flag: &str| node.children(&mut node.walk()).any(|c| c.kind() == flag);
            self.variables.push(VariableDeclaration {
                name: self.text_of(variable).trim().to_string(),
                span: self.span_of(variable),
                section: self.target.section,
                scope: if node.kind().starts_with("local") {
                    Scope::Local
                } else {
                    Scope::Global
                },
                persist: has_flag("persist"),
                locked: has_flag("locked"),
            });
        }

        let value = node
            .child_by_field_name("value")
            .or_else(|| node.child_by_field_name("expression"));
        if let Some(value) = value {
            self.visit(value);
        }
    }

    fn visit_reference(&mut self, node: Node, kind: SymbolKind) {
        let text = self.reference_text(node);
        if !text.is_empty() {
            self.references.push(Reference {
                kind,
                text,
                span: self.span_of(node),
                section: self.target.section,
                access: access(node),
            });
        }
        self.visit_children(node, |child| REFERENCE_CHILDREN.contains(&child.kind()));
    }

    /// Reference text as written; a pool's index is not part of its name.
    fn reference_text(&self, node: Node) -> String {
        let mut cursor = node.walk();
        let index = node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "index_expression");
        let end = index.map_or(node.end_byte(), |i| i.start_byte());
        self.source[node.start_byte()..end].trim().to_string()
    }

    fn text_of(&self, node: Node) -> &str {
        &self.source[node.start_byte()..node.end_byte()]
    }

    fn span_of(&self, node: Node) -> Span {
        Span {
            start: node.start_byte() + self.offset,
            end: node.end_byte() + self.offset,
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

fn access(node: Node) -> Access {
    // `$PoolFoo[0] = ...` wraps the pool in a pooled_variable.
    let target = match node.parent() {
        Some(parent) if parent.kind() == "pooled_variable" => parent,
        _ => node,
    };
    let Some(parent) = target.parent() else {
        return Access::Read;
    };

    if ASSIGNMENTS.contains(&parent.kind()) && parent.child_by_field_name("name") == Some(target) {
        return Access::Write;
    }
    if parent.kind() == "store_instruction" {
        let mut cursor = parent.walk();
        let written = parent
            .named_children(&mut cursor)
            .find(|c| c.kind() != "instruction");
        if written == Some(target) {
            return Access::Write;
        }
    }
    Access::Read
}
