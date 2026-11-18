//! XM108: a command written with the wrong number of comma-separated arguments.

use super::{Finding, RuleContext};
use crate::spec::{MatchKind, SectionKind};

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        if section.spec.map(|s| s.kind) != Some(SectionKind::CommandList) {
            continue;
        }

        for line in &section.lines {
            if section.issues.iter().any(|i| i.line == line.scanned.line) {
                continue;
            }
            let Some(value_span) = line.scanned.value.filter(|v| v.start != v.end) else {
                continue;
            };
            let Some(command) = context
                .lookup
                .command(line.command.as_deref().unwrap_or(""))
            else {
                continue;
            };
            let Some(expected) = command.arg_count else {
                continue;
            };
            if command.match_kind != MatchKind::Exact
                || Some(&command.name) != line.command.as_ref()
            {
                continue;
            }

            let written = value_span.slice(context.text);
            let lower = written.to_lowercase();
            let special = command.values.iter().any(|v| match v.match_kind {
                MatchKind::Exact => lower == v.name,
                MatchKind::Prefix => lower.starts_with(&v.name),
            });
            if special {
                continue;
            }

            let count = argument_count(written);
            if count != expected {
                findings.push(Finding::new(
                    value_span,
                    format!(
                        "`{}` takes {expected} argument{}, got {count}",
                        command.name,
                        if expected == 1 { "" } else { "s" }
                    ),
                ));
            }
        }
    }
    findings
}

/// Commas outside parentheses/brackets: the argument separators of a command value.
fn argument_count(value: &str) -> usize {
    let mut depth = 0i32;
    let mut count = 1;
    for c in value.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas_inside_brackets_are_not_separators() {
        assert_eq!(argument_count("a, b"), 2);
        assert_eq!(argument_count("f(a, b), c[1, 2]"), 2);
        assert_eq!(argument_count("a"), 1);
    }
}
