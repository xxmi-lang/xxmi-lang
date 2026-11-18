//! XM105: a value outside the enum table the DLL parses the key against.

use super::{Finding, RuleContext, replace_fix};
use crate::spec::SectionKind;
use crate::suggest::did_you_mean;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section.spec else {
            continue;
        };
        for line in &section.lines {
            let (Some(key_span), Some(value_span)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            if value_span.start == value_span.end {
                continue;
            }

            let key_name = if spec.kind == SectionKind::CommandList {
                line.command.as_deref().unwrap_or("")
            } else {
                key_span.slice(context.text)
            };
            let Some(table) = context
                .lookup
                .key(spec, key_name)
                .and_then(|k| enum_table_name(&k.value_type))
            else {
                continue;
            };
            let Some(table) = context.lookup.enum_table(table) else {
                continue;
            };
            let values: Vec<&str> = table.values.iter().map(|v| v.name.as_str()).collect();
            if values.is_empty() {
                continue;
            }

            let written = value_span.slice(context.text);
            let value = if table.name == "DXGIFormats" {
                // ParseFormatString: numeric value, or a name with an optional DXGI_FORMAT_ prefix.
                if written.bytes().all(|b| b.is_ascii_digit()) {
                    continue;
                }
                strip_format_prefix(written)
            } else {
                written
            };

            let lower = value.to_lowercase();
            if values.iter().any(|v| v.to_lowercase() == lower) {
                continue;
            }

            let prefix = &written[..written.len() - value.len()];
            findings.push(Finding {
                span: value_span,
                message: format!(
                    "Invalid {} \"{written}\".{}",
                    key_span.slice(context.text),
                    did_you_mean(value, values.iter().copied())
                ),
                fix: replace_fix(value_span, value, &values, prefix),
            });
        }
    }
    findings
}

/// `^enum:(\w+)$`
fn enum_table_name(value_type: &str) -> Option<&str> {
    let name = value_type.strip_prefix("enum:")?;
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    (!name.is_empty() && name.chars().all(is_word)).then_some(name)
}

fn strip_format_prefix(value: &str) -> &str {
    const PREFIX: &str = "DXGI_FORMAT_";
    match value.get(..PREFIX.len()) {
        Some(head) if head.eq_ignore_ascii_case(PREFIX) => &value[PREFIX.len()..],
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_names() {
        assert_eq!(enum_table_name("enum:DXGIFormats"), Some("DXGIFormats"));
        assert_eq!(enum_table_name("enum:"), None);
        assert_eq!(enum_table_name("string"), None);
    }

    #[test]
    fn format_prefix_is_optional_and_caseless() {
        assert_eq!(strip_format_prefix("dxgi_format_R8"), "R8");
        assert_eq!(strip_format_prefix("R8"), "R8");
    }
}
