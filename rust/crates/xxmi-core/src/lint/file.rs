//! Lints one loaded file against its workspace (`lintFile` in the TypeScript engine).

use std::collections::HashSet;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Fix, TextEdit};
use crate::model::IniFile;
use crate::rules::{Finding, RULES, Rule, RuleContext};
use crate::trace;
use crate::workspace::{FileId, Workspace};

use super::config::{ConfigLoader, RuleSetting};
use super::suppress::Suppressions;
use super::{ConfigError, LintOptions};

/// Diagnostics of one file, suppressed and sorted by position, then rule id.
pub fn lint_file(
    id: FileId,
    workspace: &Workspace,
    options: &LintOptions,
    configs: &ConfigLoader,
) -> Result<Vec<Diagnostic>, ConfigError> {
    let file = &workspace.files()[id];
    let config = configs.for_directory(file.path.parent().unwrap_or(Path::new("")))?;
    let only: Option<HashSet<&str>> = options
        .rules
        .as_ref()
        .map(|rules| rules.iter().map(String::as_str).collect());

    let context = RuleContext {
        text: &file.text,
        file,
        file_id: id,
        workspace,
        lookup: workspace.lookup(),
        package_root: workspace.package_root(file),
    };

    let mut diagnostics = Vec::new();
    for rule in &RULES {
        if only.as_ref().is_some_and(|only| !only.contains(rule.id)) {
            continue;
        }
        if options
            .rule_prefixes
            .as_ref()
            .is_some_and(|prefixes| !prefixes.iter().any(|p| rule.id.starts_with(p)))
        {
            continue;
        }
        let configured = options
            .overrides
            .get(rule.id)
            .or_else(|| config.rules.get(rule.id))
            .copied();
        let configured = match configured {
            Some(RuleSetting::Off) => continue,
            Some(RuleSetting::Level(severity)) => Some(severity),
            None => None,
        };

        let findings = trace::span(rule.id, || (rule.check)(&context));
        for finding in findings {
            let severity = configured.or(finding.severity).unwrap_or(rule.severity);
            diagnostics.push(diagnostic(file, rule, severity, finding));
        }
    }

    let suppressions = Suppressions::new(file);
    diagnostics.retain(|d| !suppressions.is_suppressed(d.range.start.line, d.id));
    diagnostics.sort_by(|a, b| {
        (a.range.start.line, a.range.start.character, a.id).cmp(&(
            b.range.start.line,
            b.range.start.character,
            b.id,
        ))
    });
    Ok(diagnostics)
}

fn diagnostic(
    file: &IniFile,
    rule: &Rule,
    severity: crate::diagnostic::Severity,
    finding: Finding,
) -> Diagnostic {
    let fix = finding.fix.map(|fix| Fix {
        title: fix.title,
        edits: vec![TextEdit {
            range: file.range_of(fix.span),
            new_text: fix.new_text,
        }],
    });
    Diagnostic {
        id: rule.id,
        severity,
        message: finding.message,
        range: file.range_of(finding.span),
        fix,
    }
}
