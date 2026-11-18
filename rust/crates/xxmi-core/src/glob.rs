//! The `exclude_recursive` globs of a package, as the DLL's `glob_to_regex` compiles them (PCRE2
//! glob, caseless) and matches them against a single file or folder name.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Any,
    One,
    Literal(char),
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
}

#[derive(Debug, Clone)]
pub struct Glob {
    tokens: Vec<Token>,
}

impl Glob {
    pub fn new(glob: &str) -> Self {
        let chars: Vec<char> = glob.chars().collect();
        let mut tokens = Vec::new();

        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '*' => tokens.push(Token::Any),
                '?' => tokens.push(Token::One),
                '[' => match class_end(&chars, i) {
                    Some(end) => {
                        tokens.push(parse_class(&chars[i + 1..end]));
                        i = end;
                    }
                    None => tokens.push(Token::Literal('[')),
                },
                c => tokens.push(Token::Literal(c)),
            }
            i += 1;
        }
        Self { tokens }
    }

    pub fn is_match(&self, name: &str) -> bool {
        let chars: Vec<char> = name.chars().collect();
        matches_from(&self.tokens, &chars)
    }
}

/// The closing `]` of a class opened at `open`: searched from two characters on, so `[]` is never a
/// class.
fn class_end(chars: &[char], open: usize) -> Option<usize> {
    let from = open + 2;
    chars
        .get(from..)?
        .iter()
        .position(|&c| c == ']')
        .map(|p| from + p)
}

/// A leading `!` (or `^`) negates; `a-z` is a range.
fn parse_class(body: &[char]) -> Token {
    let negated = matches!(body.first(), Some('!' | '^'));
    let body = if negated { &body[1..] } else { body };

    let mut ranges = Vec::new();
    let mut i = 0;
    while i < body.len() {
        if i + 2 < body.len() && body[i + 1] == '-' {
            ranges.push((body[i], body[i + 2]));
            i += 3;
        } else {
            ranges.push((body[i], body[i]));
            i += 1;
        }
    }
    Token::Class { negated, ranges }
}

fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(only), None) => only,
        _ => c,
    }
}

fn matches_from(tokens: &[Token], name: &[char]) -> bool {
    let Some((token, rest)) = tokens.split_first() else {
        return name.is_empty();
    };

    match token {
        Token::Any => (0..=name.len()).any(|skip| matches_from(rest, &name[skip..])),
        Token::One => !name.is_empty() && matches_from(rest, &name[1..]),
        Token::Literal(c) => {
            name.first().is_some_and(|&n| fold(n) == fold(*c)) && matches_from(rest, &name[1..])
        }
        Token::Class { negated, ranges } => {
            let Some(&n) = name.first() else {
                return false;
            };
            let n = fold(n);
            let inside = ranges
                .iter()
                .any(|&(lo, hi)| fold(lo) <= n && n <= fold(hi));
            inside != *negated && matches_from(rest, &name[1..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_and_question_mark() {
        let glob = Glob::new("DISABLED*");
        assert!(glob.is_match("disabled mod"));
        assert!(glob.is_match("DISABLED"));
        assert!(!glob.is_match("not DISABLED"));
        assert!(Glob::new("desktop.ini").is_match("Desktop.INI"));
        assert!(!Glob::new("desktop.ini").is_match("desktopxini"));
        assert!(Glob::new("a?c").is_match("abc"));
    }

    #[test]
    fn classes_and_negation() {
        assert!(Glob::new("[a-c]x").is_match("bx"));
        assert!(!Glob::new("[a-c]x").is_match("dx"));
        assert!(Glob::new("[!a-c]x").is_match("dx"));
        assert!(Glob::new("[abc").is_match("[abc"));
    }
}
