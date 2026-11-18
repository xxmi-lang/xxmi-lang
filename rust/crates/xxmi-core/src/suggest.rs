//! "Did you mean ..." suggestions by edit distance (port of packages/core/src/lint/suggest.ts).
//!
//! Distances count UTF-16 code units, like the JavaScript original.

fn utf16_lower(text: &str) -> Vec<u16> {
    text.to_lowercase().encode_utf16().collect()
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let (x, y) = (utf16_lower(a), utf16_lower(b));
    let mut prev: Vec<usize> = (0..=y.len()).collect();

    for (i, cx) in x.iter().enumerate() {
        let mut row = vec![i + 1];
        for (j, cy) in y.iter().enumerate() {
            let cost = usize::from(cx != cy);
            row.push((prev[j + 1] + 1).min(row[j] + 1).min(prev[j] + cost));
        }
        prev = row;
    }
    prev[y.len()]
}

/// The closest candidate within a distance that scales with length, or `None`.
pub fn nearest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (word.encode_utf16().count() / 3).clamp(1, 3);
    let mut best: Option<&str> = None;
    let mut best_distance = limit + 1;

    for candidate in candidates {
        let distance = edit_distance(word, candidate);
        let closer = distance < best_distance;
        let ties_alphabetically = distance == best_distance && best.is_some_and(|b| candidate < b);
        if closer || ties_alphabetically {
            best = Some(candidate);
            best_distance = distance;
        }
    }
    (best_distance <= limit).then_some(best).flatten()
}

pub fn did_you_mean<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    match nearest(word, candidates) {
        Some(found) => format!(" Did you mean `{found}`?"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_ignores_case() {
        assert_eq!(edit_distance("Stride", "strid"), 1);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    #[test]
    fn nearest_prefers_the_alphabetically_first_tie() {
        assert_eq!(
            nearest("strid", ["strideb", "stride", "strida"]),
            Some("strida")
        );
        assert_eq!(nearest("zzzzzz", ["stride"]), None);
    }

    #[test]
    fn suggestion_text_matches_the_typescript_message() {
        assert_eq!(did_you_mean("strid", ["stride"]), " Did you mean `stride`?");
        assert_eq!(did_you_mean("zzzzzz", ["stride"]), "");
    }
}
