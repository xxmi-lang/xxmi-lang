//! Line-start keyword tests shared by the syntax and flow rules.

use crate::model::{LineModel, SectionModel};
use crate::spec::SectionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowKeyword {
    If,
    Elif,
    Else,
    Endif,
}

impl FlowKeyword {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::If => "if",
            Self::Elif => "elif",
            Self::Else => "else",
            Self::Endif => "endif",
        }
    }
}

/// Flow-control keyword a command-list line starts with (`ParseCommandListFlowControl`).
pub fn flow_keyword(line: &LineModel, text: &str) -> Option<FlowKeyword> {
    let first = text.as_bytes()[line.scanned.content.start].to_ascii_lowercase();
    if first != b'i' && first != b'e' {
        return None;
    }
    let content = line.scanned.content.slice(text).to_lowercase();

    if starts_with_keyword(&content, &["if"]) {
        Some(FlowKeyword::If)
    } else if starts_with_keyword(&content, &["elif", "else if"]) {
        Some(FlowKeyword::Elif)
    } else if content == "else" {
        Some(FlowKeyword::Else)
    } else if content == "endif" {
        Some(FlowKeyword::Endif)
    } else {
        None
    }
}

/// A flow keyword where the DLL doesn't read flow control: the preamble (`None`)
/// or a section that isn't a command list (`ParseCommandList` only runs on those).
/// Unknown sections are left to XM101.
pub fn is_misplaced_flow(section: Option<&SectionModel>, line: &LineModel, text: &str) -> bool {
    let outside = match section {
        None => true,
        Some(section) => section
            .spec
            .is_some_and(|spec| spec.kind != SectionKind::CommandList),
    };
    outside && flow_keyword(line, text).is_some()
}

/// Lines whose value the DLL parses as an expression (`CommandListExpression::parse`).
pub fn is_expression_line(line: Option<&LineModel>, text: &str) -> bool {
    let Some(line) = line else {
        return false;
    };
    let content = line.scanned.content.slice(text).to_lowercase();

    starts_with_keyword(&content, &["if", "elif", "else if"])
        || starts_with_variable(&content)
        || content.starts_with("local ")
}

/// `^(if|elif|else if)\s`
pub fn starts_with_keyword(content: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| {
        content
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
    })
}

/// `^(local\s+)?\$`
pub fn starts_with_variable(content: &str) -> bool {
    let rest = match content.strip_prefix("local") {
        Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
        _ => content,
    };
    rest.starts_with('$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_prefixes() {
        assert!(starts_with_keyword("if $a == 1", &["if"]));
        assert!(starts_with_keyword("else if x", &["if", "elif", "else if"]));
        assert!(!starts_with_keyword("iffy = 1", &["if"]));
        assert!(starts_with_variable("$a = 1"));
        assert!(starts_with_variable("local $a = 1"));
        assert!(!starts_with_variable("local a = 1"));
    }
}
