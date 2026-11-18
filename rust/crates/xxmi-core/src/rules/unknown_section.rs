//! XM101: a section prefix the DLL doesn't know.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::{Finding, RuleContext};
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
            let hint = match suggest_cached(spec, name, &names) {
                Some(found) => format!(" Did you mean a [{found}…] section?"),
                None => String::new(),
            };
            Finding::new(
                section.name,
                format!("Unknown section type [{name}].{hint}"),
            )
        })
        .collect()
}

/// Distinct misspelt names a session keeps suggestions for before starting over.
const CACHE_LIMIT: usize = 1024;

/// A misspelt header stays in the file edit after edit, and `suggest` is the costliest thing a
/// lint does with it. `spec` keys the entry to the loaded spec, whose names the answer depends on.
fn suggest_cached(spec: &Spec, name: &str, names: &[&str]) -> Option<String> {
    thread_local! {
        static SUGGESTIONS: RefCell<HashMap<(usize, String), Option<String>>> =
            RefCell::new(HashMap::new());
    }
    let key = (std::ptr::from_ref(spec) as usize, name.to_string());
    SUGGESTIONS.with_borrow_mut(|cache| {
        if let Some(found) = cache.get(&key) {
            return found.clone();
        }
        let found = suggest(name, names).map(str::to_string);
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, found.clone());
        found
    })
}

/// Compares each known prefix with the name's leading characters, allowing a couple of missing or
/// extra letters (`[TextureOverid…]`).
fn suggest<'a>(name: &str, names: &[&'a str]) -> Option<&'a str> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut scored: Vec<(usize, &str)> = names
        .iter()
        .filter_map(|&known| {
            let distance = prefix_distance(&units, known);
            let limit = 2.min(known.encode_utf16().count() / 3);
            (distance > 0 && distance <= limit).then_some((distance, known))
        })
        .collect();

    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| locale_compare(a.1, b.1)));
    scored.first().map(|(_, known)| *known)
}

fn prefix_distance(units: &[u16], known: &str) -> usize {
    let length = known.encode_utf16().count();
    (-2i64..=2)
        .map(|delta| length as i64 + delta)
        .filter(|&l| l > 0)
        .map(|l| {
            let end = (l as usize).min(units.len());
            edit_distance(&String::from_utf16_lossy(&units[..end]), known)
        })
        .min()
        .unwrap_or(usize::MAX)
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
