//! XM210: a local declared again where an earlier local of the same name is visible.

use std::collections::HashMap;

use super::global_redeclared::Redeclaration;
use super::keywords::{FlowKeyword, flow_keyword};
use super::{Finding, RuleContext};
use crate::model::{IniFile, Scope, SectionModel};
use crate::spec::SectionKind;

/// declare_local_variable (CommandList.cpp) refuses a name that find_local_variable already sees
/// in the section's scope stack: sibling branches may reuse a name, a nested one may not shadow it.
/// `if` opens a scope; `elif` and `else` clear the deepest one; `endif` closes it.
pub fn redeclared_locals(file: &IniFile, text: &str) -> Vec<Redeclaration> {
    let mut redeclared = Vec::new();
    for (index, section) in file.sections.iter().enumerate() {
        if section.spec.map(|s| s.kind) != Some(SectionKind::CommandList) {
            continue;
        }

        let locals: Vec<usize> = (0..file.variables.len())
            .filter(|&i| {
                file.variables[i].scope == Scope::Local && file.variables[i].section == Some(index)
            })
            .collect();
        redeclared.extend(section_redeclarations(file, text, section, &locals));
    }
    redeclared
}

fn section_redeclarations(
    file: &IniFile,
    text: &str,
    section: &SectionModel,
    locals: &[usize],
) -> Vec<Redeclaration> {
    let mut scopes: Vec<HashMap<String, usize>> = vec![HashMap::new()];
    let mut redeclared = Vec::new();
    let mut next = 0;

    for line in &section.lines {
        match flow_keyword(line, text) {
            Some(FlowKeyword::If) => scopes.push(HashMap::new()),
            Some(FlowKeyword::Elif | FlowKeyword::Else) if scopes.len() > 1 => {
                scopes.pop();
                scopes.push(HashMap::new());
            }
            Some(FlowKeyword::Endif) if scopes.len() > 1 => {
                scopes.pop();
            }
            _ => {}
        }

        while let Some(&index) = locals.get(next) {
            let declaration = &file.variables[index];
            if declaration.span.start >= line.scanned.content.end {
                break;
            }
            next += 1;

            let lower = declaration.name.to_lowercase();
            match scopes.iter().find_map(|scope| scope.get(&lower)) {
                Some(&first) => redeclared.push(Redeclaration { index, first }),
                None => {
                    if let Some(scope) = scopes.last_mut() {
                        scope.insert(lower, index);
                    }
                }
            }
        }
    }
    redeclared
}

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let file = context.file;
    redeclared_locals(file, context.text)
        .into_iter()
        .map(|r| {
            let variable = &file.variables[r.index];
            let first_line = file.range_of(file.variables[r.first].span).start.line + 1;
            Finding::new(
                variable.span,
                format!(
                    "Local {} is already declared on line {first_line} in this or an enclosing branch; the DLL rejects this redeclaration.",
                    variable.name
                ),
            )
        })
        .collect()
}
