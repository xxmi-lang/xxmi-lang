//! The tree-sitter parser for the grammar and the ERROR/MISSING collection behind XM001
//! (port of `collectSyntaxIssues` in packages/core/src/parser/index.ts).

use tree_sitter::{Node, Parser, Tree};

use crate::scan::Span;

/// First non-blank line of a snippet is quoted up to this many UTF-16 code units.
const SNIPPET_LENGTH: usize = 60;

pub fn new_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_migoto::LANGUAGE.into())
        .expect("the vendored grammar matches the tree-sitter runtime");
    parser
}

/// A node the grammar could not fit into the tree, with offsets local to the parsed text.
#[derive(Debug, Clone)]
pub struct SyntaxIssue {
    pub span: Span,
    /// Row the issue starts on, local to the parsed text.
    pub row: usize,
    pub message: String,
}

/// The outermost ERROR nodes and all MISSING nodes, in document order.
pub fn collect_syntax_issues(tree: &Tree, source: &str) -> Vec<SyntaxIssue> {
    let mut issues = Vec::new();
    let root = tree.root_node();
    if root.has_error() {
        visit(root, source, &mut issues);
    }
    issues
}

fn visit(node: Node, source: &str, issues: &mut Vec<SyntaxIssue>) {
    if node.is_missing() {
        issues.push(issue(node, format!("Missing {}", describe_missing(node))));
        return;
    }
    if node.is_error() {
        let text = node.utf8_text(source.as_bytes()).unwrap_or_default();
        issues.push(issue(
            node,
            format!("Syntax error: unexpected {}", quote_snippet(text)),
        ));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.has_error() || child.is_missing() {
            visit(child, source, issues);
        }
    }
}

fn issue(node: Node, message: String) -> SyntaxIssue {
    SyntaxIssue {
        span: Span {
            start: node.start_byte(),
            end: node.end_byte(),
        },
        row: node.start_position().row,
        message,
    }
}

fn describe_missing(node: Node) -> String {
    if node.is_named() {
        node.kind().to_string()
    } else {
        format!("\"{}\"", node.kind())
    }
}

/// First non-blank line of `text`, shortened, in backticks.
fn quote_snippet(text: &str) -> String {
    let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty()) else {
        return "end of line".to_string();
    };

    let mut units = 0;
    let mut shortened = String::new();
    for c in line.chars() {
        units += c.len_utf16();
        if units > SNIPPET_LENGTH {
            return format!("`{shortened}…`");
        }
        shortened.push(c);
    }
    format!("`{line}`")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues(text: &str) -> Vec<String> {
        let tree = new_parser().parse(text, None).expect("parses");
        collect_syntax_issues(&tree, text)
            .into_iter()
            .map(|i| i.message)
            .collect()
    }

    #[test]
    fn clean_text_has_no_issues() {
        assert!(issues("[Constants]\nglobal $a = 1\n").is_empty());
    }

    #[test]
    fn garbage_line_is_quoted() {
        let found = issues("[Constants]\nglobal $ok = 1\nthis line is not valid");
        assert_eq!(found, ["Syntax error: unexpected `this line is not valid`"]);
    }

    #[test]
    fn long_snippets_are_shortened() {
        let line = "x".repeat(80);
        assert_eq!(
            quote_snippet(&line),
            format!("`{}…`", "x".repeat(SNIPPET_LENGTH))
        );
    }
}
