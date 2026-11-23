//! The lint rules, in the order the engine runs them (packages/core/src/lint/engine.ts).

mod argument_count;
mod bare_line;
mod duplicate_grade;
mod duplicate_key;
mod duplicate_section;
mod empty_value;
mod enum_value;
mod flow_balance;
mod flow_case;
mod flow_outside;
mod global_redeclared;
mod hash_format;
mod header;
mod inline_comment;
mod keywords;
mod local_redeclared;
mod missing_file;
mod missing_include;
mod no_hash;
mod outside_section;
mod setting_inside_if;
mod syntax;
mod unknown_key;
mod unknown_namespace;
mod unknown_section;
mod unknown_variable;
mod unresolved;
mod unused_section;
mod unused_variable;

use std::path::Path;

use crate::diagnostic::Severity;
use crate::lookup::Lookup;
use crate::model::IniFile;
use crate::scan::Span;
use crate::suggest::nearest;
use crate::workspace::symbols::SymbolTable;
use crate::workspace::{FileId, Workspace};

pub struct RuleContext<'a> {
    pub text: &'a str,
    pub file: &'a IniFile,
    pub file_id: FileId,
    pub workspace: &'a Workspace,
    pub lookup: &'static Lookup,
    /// The 3DMigoto folder the file was loaded under, if found.
    pub package_root: Option<&'a Path>,
}

impl RuleContext<'_> {
    pub fn symbols(&self) -> &SymbolTable {
        self.workspace.symbols()
    }

    /// Files under a package's `Core/` folder are libraries other mods call into.
    pub fn is_library(&self) -> bool {
        let Some(root) = self.package_root else {
            return false;
        };
        self.file
            .path
            .strip_prefix(root)
            .ok()
            .and_then(|relative| relative.components().next())
            .is_some_and(|first| first.as_os_str().to_string_lossy().to_lowercase() == "core")
    }
}

/// A rule's report, still in byte offsets.
pub struct Finding {
    pub span: Span,
    pub message: String,
    pub fix: Option<FixSpec>,
    /// Overrides the rule's default severity, unless the user configured one.
    pub severity: Option<Severity>,
}

impl Finding {
    pub fn new(span: Span, message: String) -> Self {
        Self {
            span,
            message,
            fix: None,
            severity: None,
        }
    }
}

/// A "replace this span with the suggestion" quick fix.
pub struct FixSpec {
    pub title: String,
    pub span: Span,
    pub new_text: String,
    /// More edits applied together with the main one, as (span, new text).
    pub also: Vec<(Span, String)>,
}

/// A "replace with the suggestion" quick fix, if there is a suggestion.
fn replace_fix(span: Span, word: &str, candidates: &[&str], prefix: &str) -> Option<FixSpec> {
    let found = nearest(word, candidates.iter().copied())?;
    Some(FixSpec {
        title: format!("Change to `{prefix}{found}`"),
        span,
        new_text: format!("{prefix}{found}"),
        also: Vec::new(),
    })
}

/// A "comment out the line" quick fix, for lines the DLL warns about and drops.
fn comment_out_fix(content: Span) -> FixSpec {
    FixSpec {
        title: "Comment out the line".to_string(),
        span: Span {
            start: content.start,
            end: content.start,
        },
        new_text: "; ".to_string(),
        also: Vec::new(),
    }
}

pub struct Rule {
    pub id: &'static str,
    pub severity: Severity,
    /// One line, shown by `xxmi lint --help`.
    pub description: &'static str,
    /// Whether the check can attach a quick fix to its findings.
    pub has_fix: bool,
    pub check: fn(&RuleContext) -> Vec<Finding>,
}

const fn rule(
    id: &'static str,
    severity: Severity,
    description: &'static str,
    check: fn(&RuleContext) -> Vec<Finding>,
) -> Rule {
    Rule {
        id,
        severity,
        description,
        has_fix: false,
        check,
    }
}

impl Rule {
    const fn with_fix(mut self) -> Self {
        self.has_fix = true;
        self
    }
}

pub const RULES: [Rule; 31] = [
    rule(
        "XM001",
        Severity::Error,
        "Parse error (tree-sitter ERROR/MISSING node)",
        syntax::check_parse_errors,
    ),
    rule(
        "XM002",
        Severity::Warning,
        "File isn't valid UTF-8 (likely GBK/Shift-JIS)",
        syntax::check_encoding,
    ),
    rule(
        "XM003",
        Severity::Warning,
        "Preamble line other than `namespace` / `condition` (the DLL ignores it)",
        outside_section::check,
    )
    .with_fix(),
    rule(
        "XM005",
        Severity::Warning,
        "Text after the `]` of a section header (the DLL ignores it)",
        header::check_suffix,
    )
    .with_fix(),
    rule(
        "XM006",
        Severity::Warning,
        "Section header with no closing `]`",
        header::check_unclosed,
    )
    .with_fix(),
    rule(
        "XM101",
        Severity::Error,
        "Unknown section prefix (not in the command-list or regular section tables)",
        unknown_section::check,
    ),
    rule(
        "XM102",
        Severity::Warning,
        "Unknown key for this section kind",
        unknown_key::check,
    )
    .with_fix(),
    rule(
        "XM103",
        Severity::Error,
        "Bare line (no `=`) in a section that doesn't allow it",
        bare_line::check,
    )
    .with_fix(),
    rule(
        "XM104",
        Severity::Warning,
        "Duplicate key not in the DLL's duplicate whitelist",
        duplicate_key::check,
    ),
    rule(
        "XM105",
        Severity::Error,
        "Invalid enum value (resource `type`, `format`, key `type`, …)",
        enum_value::check,
    )
    .with_fix(),
    rule(
        "XM106",
        Severity::Error,
        "Unbalanced `if`/`elif`/`else`/`endif`",
        flow_balance::check,
    )
    .with_fix(),
    rule(
        "XM107",
        Severity::Warning,
        "Duplicate section name within the same namespace",
        duplicate_section::check,
    ),
    rule(
        "XM108",
        Severity::Error,
        "Wrong argument count for a command (`store`, draw commands, …)",
        argument_count::check,
    ),
    rule(
        "XM109",
        Severity::Error,
        "Unknown operator or malformed expression",
        syntax::check_expressions,
    ),
    rule(
        "XM110",
        Severity::Warning,
        "Key with no value (the DLL ignores the line)",
        empty_value::check,
    )
    .with_fix(),
    rule(
        "XM111",
        Severity::Error,
        "`if`/`elif`/`else`/`endif` outside a command list",
        flow_outside::check,
    ),
    rule(
        "XM112",
        Severity::Warning,
        "TextureOverride with no `hash` and no resource-description `match_*` key",
        no_hash::check,
    ),
    rule(
        "XM113",
        Severity::Error,
        "Hash value the DLL can't read as hex, or that isn't canonical (error or warning per finding)",
        hash_format::check,
    ),
    rule(
        "XM114",
        Severity::Info,
        "TextureOverride / ShaderOverride setting written inside an `if` (the DLL reads it unconditionally)",
        setting_inside_if::check,
    )
    .with_fix(),
    rule(
        "XM115",
        Severity::Warning,
        "`;` comment after a value (the DLL keeps it in the value)",
        inline_comment::check,
    )
    .with_fix(),
    rule(
        "XM201",
        Severity::Error,
        "Unresolved `run = CommandList…` / `CustomShader…` target",
        unresolved::check_commands,
    )
    .with_fix(),
    rule(
        "XM202",
        Severity::Error,
        "Unresolved resource reference (`ref`, `copy`, `Resource\\N\\X`, pools)",
        unresolved::check_resources,
    )
    .with_fix(),
    rule(
        "XM203",
        Severity::Warning,
        "Variable used but never declared",
        unknown_variable::check,
    ),
    rule(
        "XM204",
        Severity::Warning,
        "Variable declared but never used",
        unused_variable::check,
    ),
    rule(
        "XM205",
        Severity::Info,
        "Section defined but never referenced (CommandList, Resource, CustomShader)",
        unused_section::check,
    ),
    rule(
        "XM206",
        Severity::Error,
        "Unresolved namespace in `\\N\\name`",
        unknown_namespace::check,
    ),
    rule(
        "XM207",
        Severity::Error,
        "Include target file not found (`include =`)",
        missing_include::check,
    ),
    rule(
        "XM208",
        Severity::Warning,
        "`filename =` points to a missing file",
        missing_file::check,
    ),
    rule(
        "XM209",
        Severity::Warning,
        "Global declared twice in [Constants] (the DLL keeps the first)",
        global_redeclared::check,
    ),
    rule(
        "XM210",
        Severity::Error,
        "Local redeclared where an earlier local of the same name is visible (the DLL drops the line)",
        local_redeclared::check,
    ),
    rule(
        "XM403",
        Severity::Hint,
        "Flow keyword (`if`, `elif`, `else`, `endif`) that isn't lowercase",
        flow_case::check,
    )
    .with_fix(),
];
