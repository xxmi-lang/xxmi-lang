//! Walks one chunk's tree for statements, references and variable declarations.

use tree_sitter::Node;

use crate::document::Chunk;
use crate::parse::{SyntaxIssue, collect_syntax_issues};
use crate::scan::Span;

use super::types::{Access, Reference, Scope, SymbolKind, Usage, UsageKind, VariableDeclaration};

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

/// What one chunk's tree says, with spans and rows local to the chunk and no section set. A chunk
/// whose text is unchanged keeps its analysis when an edit moves it elsewhere in the file.
#[derive(Default)]
pub struct ChunkAnalysis {
    pub issues: Vec<SyntaxIssue>,
    /// Chunk-local row and node kind of each statement, in document order.
    pub statements: Vec<(u32, String)>,
    pub references: Vec<Reference>,
    pub usages: Vec<Usage>,
    pub variables: Vec<VariableDeclaration>,
}

pub(super) fn analyse_chunk(chunk: &Chunk, source: &str) -> ChunkAnalysis {
    let mut analysis = ChunkAnalysis {
        issues: collect_syntax_issues(&chunk.tree, source),
        ..ChunkAnalysis::default()
    };
    let mut walker = Walker {
        source,
        analysis: &mut analysis,
    };
    walker.visit(chunk.tree.root_node());
    analysis
}

struct Walker<'a> {
    source: &'a str,
    analysis: &'a mut ChunkAnalysis,
}

impl Walker<'_> {
    fn visit(&mut self, node: Node) {
        // Text inside an ERROR is reported as XM001, not interpreted.
        if node.is_error() {
            return;
        }
        self.record_statement(node);
        self.record_usage(node);

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

        self.analysis
            .statements
            .push((node.start_position().row as u32, kind.to_string()));
    }

    fn record_usage(&mut self, node: Node) {
        let Some((kind, named)) = usage_of(node) else {
            return;
        };
        self.push_usage(kind, named);
    }

    fn push_usage(&mut self, kind: UsageKind, node: Node) {
        self.analysis.usages.push(Usage {
            kind,
            text: self.text_of(node).trim().to_string(),
            span: self.span_of(node),
            section: None,
        });
    }

    fn visit_declaration(&mut self, node: Node) {
        let mut cursor = node.walk();
        for keyword in node.children(&mut cursor).filter(|c| is_keyword(*c)) {
            self.push_usage(UsageKind::Modifier, keyword);
        }

        if let Some(variable) = node.child_by_field_name("variable") {
            let has_flag = |flag: &str| node.children(&mut node.walk()).any(|c| c.kind() == flag);
            self.analysis.variables.push(VariableDeclaration {
                name: self.text_of(variable).trim().to_string(),
                span: self.span_of(variable),
                section: None,
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
            self.analysis.references.push(Reference {
                kind,
                text,
                span: self.span_of(node),
                section: None,
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
            start: node.start_byte(),
            end: node.end_byte(),
        }
    }
}

/// The node whose text names what `node` uses, for the constructs the spec has entries for.
fn usage_of(node: Node) -> Option<(UsageKind, Node)> {
    let child = |kind: &str| {
        let mut cursor = node.walk();
        node.named_children(&mut cursor).find(|c| c.kind() == kind)
    };
    match node.kind() {
        "builtin_function" => child("function_name").map(|n| (UsageKind::Function, n)),
        "property_access_expression" => child("resource_property").map(|n| (UsageKind::Member, n)),
        "binary_expression" | "unary_expression" => {
            let mut cursor = node.walk();
            let operator = node.children(&mut cursor).find(|c| !c.is_named());
            operator.map(|n| (UsageKind::Operator, n))
        }
        "runtime_parameter" => Some((UsageKind::RuntimeValue, node)),
        "run_instruction_key_value" => Some((UsageKind::RunValue, node)),
        _ => None,
    }
}

/// An unnamed node spelt as a word, such as `global` or `locked` in a declaration.
fn is_keyword(node: Node) -> bool {
    !node.is_named() && node.kind().chars().all(|c| c.is_ascii_alphabetic())
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
