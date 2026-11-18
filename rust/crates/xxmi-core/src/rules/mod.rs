//! The lint rules ported so far: XM001 (syntax) and XM102 (unknown key).

mod syntax;
mod unknown_key;

use crate::diagnostic::Severity;
use crate::lookup::Lookup;
use crate::model::Model;
use crate::scan::Span;

pub struct RuleContext<'a> {
    pub text: &'a str,
    pub model: &'a Model,
    pub lookup: &'static Lookup,
}

/// A rule's report, still in byte offsets.
pub struct Finding {
    pub span: Span,
    pub message: String,
    pub fix: Option<FixSpec>,
}

/// A "replace this span with the suggestion" quick fix.
pub struct FixSpec {
    pub title: String,
    pub span: Span,
    pub new_text: String,
}

pub struct Rule {
    pub id: &'static str,
    pub severity: Severity,
    pub check: fn(&RuleContext) -> Vec<Finding>,
}

pub const RULES: [Rule; 2] = [
    Rule {
        id: "XM001",
        severity: Severity::Error,
        check: syntax::check,
    },
    Rule {
        id: "XM102",
        severity: Severity::Warning,
        check: unknown_key::check,
    },
];
