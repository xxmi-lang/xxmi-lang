//! The source side of a resource copy (`dst = [options] Resource…`), as
//! `parse_resource_copy_target_source` reads it: option words and the target in any order.

/// `ResourceCopyOptionNames` (CommandList.h).
const OPTIONS: [&str; 11] = [
    "copy",
    "ref",
    "reference",
    "copy_desc",
    "copy_description",
    "unless_null",
    "mono",
    "set_viewport",
    "no_view_cache",
    "raw",
    "resolve_msaa",
];

/// Options that choose between a copy and a reference (`COPY_TYPE_MASK`).
const TYPE_OPTIONS: [&str; 5] = ["copy", "ref", "reference", "copy_desc", "copy_description"];

fn is_one_of(token: &str, words: &[&str]) -> bool {
    words.iter().any(|word| token.eq_ignore_ascii_case(word))
}

/// Whether the value says `ref`, `copy` or `copy_desc`, so the DLL doesn't guess.
pub fn names_copy_type(value: &str) -> bool {
    value
        .split_whitespace()
        .any(|token| is_one_of(token, &TYPE_OPTIONS))
}

/// The custom resource the value copies from: its first token that isn't an option,
/// when that is spelled `Resource…`.
pub fn resource_source(value: &str) -> Option<&str> {
    let target = value
        .split_whitespace()
        .find(|token| !is_one_of(token, &OPTIONS))?;
    let prefix = target.get(.."Resource".len())?;
    prefix.eq_ignore_ascii_case("Resource").then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_come_in_any_order() {
        assert_eq!(resource_source("unless_null ResourceA"), Some("ResourceA"));
        assert_eq!(resource_source("ResourceA ref"), Some("ResourceA"));
        assert!(names_copy_type("unless_null REF ResourceA"));
        assert!(!names_copy_type("unless_null ResourceA"));
    }

    #[test]
    fn only_custom_resources_are_sources() {
        assert_eq!(resource_source("null"), None);
        assert_eq!(resource_source("ps-t0"), None);
        assert_eq!(resource_source("ref"), None);
        assert_eq!(resource_source("Res"), None);
    }
}
