use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::cpp::{
    AnchorNotFound, CppFile, find_calls, split_top_level, string_literal, string_literals,
};
use crate::dll::Extraction;
use crate::model::{
    SpecEnum, SpecEnumValue, SpecOperatorToken, SpecOperators, SpecPrecedenceLevel,
};

const CL: &str = "CommandList.cpp";

pub type OperatorPatterns = HashMap<String, String>;

static IDENTIFIER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());

/// Whether an operator pattern is a function name (`sin`) rather than a token (`+`).
pub fn is_function_name(pattern: &str) -> bool {
    IDENTIFIER.is_match(pattern)
}

/// `DEFINE_OPERATOR(name, "pattern", fn)` → name ↦ pattern.
pub fn operator_patterns(file: &CppFile) -> Result<OperatorPatterns, AnchorNotFound> {
    let mut patterns = OperatorPatterns::new();
    for call in find_calls(&file.code, "DEFINE_OPERATOR") {
        let name = call.arg(0);
        if let Some(value) = string_literal(call.arg(1))
            && !name.is_empty()
            && !patterns.contains_key(name)
        {
            patterns.insert(name.to_string(), value);
        }
    }
    if patterns.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            "DEFINE_OPERATOR(…) definitions",
            "",
        ));
    }
    Ok(patterns)
}

/// The patterns of the operators listed in the `group` array, by their `&name` references.
pub fn group_patterns<'a>(
    file: &CppFile,
    group: &str,
    patterns: &'a OperatorPatterns,
) -> Result<Vec<&'a str>, AnchorNotFound> {
    Ok(split_top_level(file.array_initializer(group)?)
        .into_iter()
        .filter_map(|reference| patterns.get(reference.strip_prefix('&').unwrap_or(reference)))
        .map(String::as_str)
        .collect())
}

/// Precedence from the order of `transform_operators_recursive(&tree, <group>, …, right, unary)`
/// calls: the expression parser folds each group in turn, tightest first.
fn precedence(
    file: &CppFile,
    patterns: &OperatorPatterns,
) -> Result<Vec<SpecPrecedenceLevel>, AnchorNotFound> {
    let mut levels: Vec<SpecPrecedenceLevel> = Vec::new();
    for call in find_calls(&file.code, "transform_operators_recursive") {
        if call.arg(0) != "&tree" {
            continue;
        }
        let group = call.arg(1);
        if levels.iter().any(|l| l.group == group) {
            continue;
        }
        let operators = group_patterns(file, group, patterns)?
            .into_iter()
            .filter(|p| !is_function_name(p))
            .map(str::to_string)
            .collect();
        levels.push(SpecPrecedenceLevel {
            level: levels.len(),
            group: group.to_string(),
            associativity: if call.arg(3) == "true" {
                "right"
            } else {
                "left"
            },
            unary: call.arg(4) == "true",
            operators,
            source: format!("{}:{group}", file.name),
            xxmi: false,
        });
    }
    if levels.is_empty() {
        return Err(AnchorNotFound::new(
            &file.name,
            "transform_operators_recursive(&tree, …) calls",
            "",
        ));
    }
    Ok(levels)
}

pub fn extract_operators(
    x: &Extraction,
) -> Result<(SpecOperators, OperatorPatterns), AnchorNotFound> {
    let file = x.dll.file(CL)?;
    let tokens: Vec<SpecOperatorToken> =
        string_literals(file.array_initializer("operator_tokens")?)
            .into_iter()
            .map(|token| SpecOperatorToken {
                token,
                source: format!("{CL}:operator_tokens"),
                xxmi: false,
                docs: Default::default(),
            })
            .collect();
    if tokens.is_empty() {
        return Err(AnchorNotFound::new(CL, "operator_tokens", "empty"));
    }
    let patterns = operator_patterns(file)?;
    let precedence = precedence(file, &patterns)?;
    Ok((SpecOperators { tokens, precedence }, patterns))
}

fn enum_values(file: &CppFile, name: &str) -> Result<Vec<SpecEnumValue>, AnchorNotFound> {
    let source = format!("{}:{name}", file.name);
    Ok(split_top_level(file.array_initializer(name)?)
        .into_iter()
        .filter_map(|entry| {
            let inner = entry.strip_prefix('{').unwrap_or(entry);
            let inner = inner.strip_suffix('}').unwrap_or(inner);
            string_literal(split_top_level(inner).first().copied().unwrap_or(""))
        })
        .map(|name| SpecEnumValue {
            name,
            source: source.clone(),
            xxmi: false,
        })
        .collect())
}

/// Every `EnumName_t<…> XNames[] = { {L"name", …}, … }` table in the DLL.
pub fn extract_enums(x: &Extraction) -> Result<Vec<SpecEnum>, AnchorNotFound> {
    let mut enums: Vec<SpecEnum> = Vec::new();
    for file in x.dll.all() {
        for name in file.enum_table_names() {
            if enums.iter().any(|e| e.name == name) {
                continue;
            }
            enums.push(SpecEnum {
                name: name.to_string(),
                values: enum_values(file, name)?,
                source: format!("{}:{name}", file.name),
                xxmi: false,
                docs: Default::default(),
            });
        }
    }
    if enums.is_empty() && x.strict {
        return Err(AnchorNotFound::new("DirectX11", "EnumName_t tables", ""));
    }

    // `format =` values: ParseFormatString (util.h) accepts these names, with or without the
    // DXGI_FORMAT_ prefix, or the numeric DXGI_FORMAT value (the index in this table).
    let formats = x.step(None, || {
        let file = x.dll.file("util.h")?;
        let names = string_literals(file.array_initializer("DXGIFormats")?);
        if names.is_empty() {
            return Err(AnchorNotFound::new(&file.name, "DXGIFormats", "empty"));
        }
        let source = format!("{}:DXGIFormats", file.name);
        Ok(Some(SpecEnum {
            name: "DXGIFormats".to_string(),
            values: names
                .into_iter()
                .map(|name| SpecEnumValue {
                    name,
                    source: source.clone(),
                    xxmi: false,
                })
                .collect(),
            source,
            xxmi: false,
            docs: Default::default(),
        }))
    })?;
    enums.extend(formats);
    Ok(enums)
}
