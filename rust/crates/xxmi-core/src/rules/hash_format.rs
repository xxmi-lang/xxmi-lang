//! XM113: a `hash` value the DLL can't read, or reads from a non-canonical form.

use super::inline_comment::comment_start;
use super::{Finding, RuleContext};
use crate::diagnostic::Severity;

/// `%16llx` in GetIniHash counts the `0x` prefix in its width.
const MAX_CHARS: usize = 16;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    for section in &context.file.sections {
        let Some(spec) = section.spec else {
            continue;
        };
        let texture = spec.name == "TextureOverride";

        for line in &section.lines {
            let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            let written = value.slice(context.text);
            // XM110 and XM115 own empty values and values with a trailing `;` comment.
            if written.is_empty() || comment_start(written).is_some() {
                continue;
            }

            let is_hash = context
                .lookup
                .key(spec, key.slice(context.text))
                .is_some_and(|k| k.value_type == "hash");
            let Some((severity, message)) = classify(written, texture).filter(|_| is_hash) else {
                continue;
            };
            findings.push(Finding {
                span: value,
                message,
                fix: None,
                severity: Some(severity),
            });
        }
    }
    findings
}

/// GetIniHash reads `sscanf("%16llx%n")` and rejects anything left over,
/// so `0x` and short forms parse.
/// A TextureOverride casts the result to 32 bits, silently dropping the high digits.
fn classify(written: &str, texture: bool) -> Option<(Severity, String)> {
    let sign = written.chars().next().filter(|c| matches!(c, '+' | '-'));
    let unsigned = &written[sign.map_or(0, |_| 1)..];
    let prefixed = unsigned
        .get(..2)
        .is_some_and(|p| p.eq_ignore_ascii_case("0x"));
    let digits = if prefixed { &unsigned[2..] } else { unsigned };

    let readable = !digits.is_empty()
        && written.len() <= MAX_CHARS
        && digits.bytes().all(|b| b.is_ascii_hexdigit());
    if !readable {
        return Some((
            Severity::Error,
            format!(
                "Invalid hash \"{written}\": the DLL rejects it (Hash parse error) and treats the key as missing."
            ),
        ));
    }

    if texture && digits.len() > 8 {
        return Some((
            Severity::Error,
            format!(
                "Hash \"{written}\" has more than 8 digits: a TextureOverride hash is 32-bit, so the DLL drops the high digits."
            ),
        ));
    }

    if let Some(sign) = sign {
        return Some((Severity::Warning, signed_message(written, sign, texture)));
    }

    let canonical_length = digits.len() == 8 || (!texture && digits.len() == 16);
    if !prefixed && canonical_length {
        return None;
    }
    let expected = if texture { "8" } else { "8 or 16" };
    Some((
        Severity::Warning,
        format!(
            "Non-canonical hash \"{written}\": the DLL accepts it, but hashes are written as {expected} hex digits with no `0x` prefix."
        ),
    ))
}

/// `%llx` follows the C standard and takes an optional sign; a `-` negates the unsigned value.
fn signed_message(written: &str, sign: char, texture: bool) -> String {
    let wraps = if sign == '-' {
        " (`-` wraps the value to `ffffffff…`)"
    } else {
        ""
    };
    let expected = if texture { "8" } else { "8 or 16" };
    format!(
        "Signed hash \"{written}\": the DLL accepts the sign{wraps}, but hashes are written as {expected} hex digits with no sign or `0x` prefix."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn severity(written: &str, texture: bool) -> Option<Severity> {
        classify(written, texture).map(|(severity, _)| severity)
    }

    #[test]
    fn canonical_forms_pass() {
        assert_eq!(severity("0123ABCD", true), None);
        assert_eq!(severity("0123456789abcdef", false), None);
    }

    #[test]
    fn dll_accepted_forms_warn() {
        for written in ["0x0123abcd", "0123abc", "0X1", "-1", "+1", "-0x1"] {
            assert_eq!(
                severity(written, true),
                Some(Severity::Warning),
                "{written}"
            );
        }
        assert_eq!(severity("0123abc", false), Some(Severity::Warning));
    }

    #[test]
    fn dll_rejected_or_truncated_forms_are_errors() {
        for written in ["0123abcg", "0x", "0123456789abcdef0", "0x0123456789abcde"] {
            assert_eq!(severity(written, false), Some(Severity::Error), "{written}");
        }
        assert_eq!(severity("0123456789abcdef", true), Some(Severity::Error));
    }
}
