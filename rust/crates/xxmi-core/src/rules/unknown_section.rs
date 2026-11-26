//! XM101: a section prefix the DLL doesn't know.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::{Finding, FixSpec, RuleContext};
use crate::model::SectionModel;
use crate::scan::Span;
use crate::spec::Spec;
use crate::suggest::edit_distance;

pub fn check(context: &RuleContext) -> Vec<Finding> {
    let spec = context.lookup.spec();
    let names: Vec<&str> = spec.sections.iter().map(|s| s.name.as_str()).collect();

    context
        .file
        .sections
        .iter()
        .filter(|section| section.spec.is_none())
        .map(|section| {
            let name = context.file.slice(section.name);
            let suggestion = suggest_cached(spec, name, &names);
            let hint = match &suggestion {
                Some(found) => format!(" Did you mean a [{}…] section?", found.known),
                None => String::new(),
            };
            Finding {
                span: section.name,
                message: format!("Unknown section type [{name}].{hint}"),
                fix: suggestion.and_then(|found| rename_prefix(context, section, &found)),
                severity: None,
            }
        })
        .collect()
}

/// Swaps the misspelt prefix for the known one, keeping the rest of the name.
/// Offered only when no other prefix was as close and no section already has the new name:
/// the DLL would then warn about a duplicate section.
fn rename_prefix(
    context: &RuleContext,
    section: &SectionModel,
    found: &Suggestion,
) -> Option<FixSpec> {
    if !found.unique {
        return None;
    }

    let name = context.file.slice(section.name);
    let replaced = byte_offset(name, found.replaced)?;
    let renamed = format!("{}{}", found.known, &name[replaced..]);
    let taken = context.file.sections.iter().any(|other| {
        context
            .file
            .slice(other.name)
            .eq_ignore_ascii_case(&renamed)
    });
    if taken {
        return None;
    }

    Some(FixSpec {
        title: format!("Change to [{renamed}]"),
        span: Span {
            start: section.name.start,
            end: section.name.start + replaced,
        },
        new_text: found.known.clone(),
        also: Vec::new(),
    })
}

/// The byte offset of the UTF-16 unit at index `units`, or `None` when it falls inside a character.
fn byte_offset(text: &str, units: usize) -> Option<usize> {
    let mut seen = 0;
    for (offset, c) in text.char_indices() {
        if seen == units {
            return Some(offset);
        }
        seen += c.len_utf16();
    }
    (seen == units).then_some(text.len())
}

/// The known prefix a misspelt name most resembles.
#[derive(Clone)]
struct Suggestion {
    known: String,
    /// UTF-16 units at the start of the name that the known prefix replaces.
    replaced: usize,
    /// No other prefix was as close.
    unique: bool,
}

/// Distinct misspelt names a session keeps suggestions for before starting over.
const CACHE_LIMIT: usize = 1024;

/// A misspelt header stays in the file edit after edit, and `suggest` is the costliest thing a
/// lint does with it. `spec` keys the entry to the loaded spec, whose names the answer depends on.
fn suggest_cached(spec: &Spec, name: &str, names: &[&str]) -> Option<Suggestion> {
    thread_local! {
        static SUGGESTIONS: RefCell<HashMap<(usize, String), Option<Suggestion>>> =
            RefCell::new(HashMap::new());
    }
    let key = (std::ptr::from_ref(spec) as usize, name.to_string());
    SUGGESTIONS.with_borrow_mut(|cache| {
        if let Some(found) = cache.get(&key) {
            return found.clone();
        }
        let found = suggest(name, names);
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, found.clone());
        found
    })
}

/// Compares each known prefix with the name's leading characters, allowing a couple of missing or
/// extra letters (`[TextureOverid…]`).
fn suggest(name: &str, names: &[&str]) -> Option<Suggestion> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut scored: Vec<(usize, usize, &str)> = names
        .iter()
        .filter_map(|&known| {
            let (distance, replaced) = prefix_distance(&units, known);
            let limit = 2.min(known.encode_utf16().count() / 3);
            (distance > 0 && distance <= limit).then_some((distance, replaced, known))
        })
        .collect();

    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| locale_compare(a.2, b.2)));
    let unique = scored.get(1).is_none_or(|second| second.0 > scored[0].0);
    scored.first().map(|&(_, replaced, known)| Suggestion {
        known: known.to_string(),
        replaced,
        unique,
    })
}

/// The closest distance to `known` and how many leading units of the name gave it.
fn prefix_distance(units: &[u16], known: &str) -> (usize, usize) {
    let length = known.encode_utf16().count();
    (-2i64..=2)
        .map(|delta| length as i64 + delta)
        .filter(|&l| l > 0)
        .map(|l| {
            let end = (l as usize).min(units.len());
            let distance = edit_distance(&String::from_utf16_lossy(&units[..end]), known);
            (distance, end)
        })
        .min_by_key(|&(distance, _)| distance)
        .unwrap_or((usize::MAX, 0))
}

/// `localeCompare` for the ASCII section names: letters alphabetically ignoring case first, then
/// lower case before upper case.
fn locale_compare(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| {
        let lower_first = |c: char| !c.is_lowercase();
        a.chars().map(lower_first).cmp(b.chars().map(lower_first))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_order_ignores_case_before_breaking_ties() {
        assert_eq!(locale_compare("key", "Key"), Ordering::Less);
        assert_eq!(locale_compare("Apple", "banana"), Ordering::Less);
    }
}
