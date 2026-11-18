//! `Number(text)` as JavaScript computes it, for `--max-warnings`: the TypeScript CLI accepts
//! whatever `Number` turns into a non-negative integer (`0x10`, `1e3`, an empty string as 0).

/// `Number(text)`; `NaN` when it isn't a number.
pub fn js_number(text: &str) -> f64 {
    let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
    if text.is_empty() {
        return 0.0;
    }

    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if text.len() > 2 && text[..2].eq_ignore_ascii_case(prefix) {
            return parse_radix(&text[2..], radix);
        }
    }

    let (sign, unsigned) = match text.as_bytes()[0] {
        b'-' => (-1.0, &text[1..]),
        b'+' => (1.0, &text[1..]),
        _ => (1.0, text),
    };
    if unsigned == "Infinity" {
        return sign * f64::INFINITY;
    }
    if is_decimal_literal(unsigned) {
        return unsigned.parse::<f64>().map_or(f64::NAN, |n| sign * n);
    }
    f64::NAN
}

fn parse_radix(digits: &str, radix: u32) -> f64 {
    let mut value = 0.0;
    for c in digits.chars() {
        match c.to_digit(radix) {
            Some(digit) => value = value * f64::from(radix) + f64::from(digit),
            None => return f64::NAN,
        }
    }
    value
}

/// `digits [. digits] [e[+-]digits]` with digits on at least one side of the point.
fn is_decimal_literal(text: &str) -> bool {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (text, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());

    let mantissa_ok =
        digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_javascript() {
        assert_eq!(js_number(""), 0.0);
        assert_eq!(js_number(" 12 "), 12.0);
        assert_eq!(js_number("0x10"), 16.0);
        assert_eq!(js_number("0b11"), 3.0);
        assert_eq!(js_number("1e3"), 1000.0);
        assert_eq!(js_number("5."), 5.0);
        assert_eq!(js_number(".5"), 0.5);
        assert_eq!(js_number("-3"), -3.0);
        assert!(js_number("-0x10").is_nan());
        assert!(js_number("1_0").is_nan());
        assert!(js_number("abc").is_nan());
        assert!(js_number(".").is_nan());
        assert!(js_number("inf").is_nan());
        assert_eq!(js_number("Infinity"), f64::INFINITY);
    }
}
