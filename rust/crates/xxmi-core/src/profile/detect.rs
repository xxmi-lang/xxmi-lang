//! Which profile a package targets, when neither the user's setting nor `xxmi.toml` says.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::model::IniFile;
use crate::paths::PathResolver;
use crate::workspace::snapshot::SymbolSnapshot;

use super::{PROFILES, Profile, by_id, generic};

/// What is known about one package.
pub struct Evidence<'a> {
    /// `None` for the files that sit in no package.
    pub root: Option<&'a Path>,
    pub resolver: &'a PathResolver,
    /// The `profile` of the package's `xxmi.toml`.
    pub configured: Option<&'static Profile>,
    pub files: &'a [&'a IniFile],
    /// The libraries on offer: references are matched against their namespaces.
    pub snapshots: &'a [SymbolSnapshot],
}

/// `xxmi.toml`, then the package's folder, then what its files reference; the generic profile
/// when none of them decides.
pub fn detect(evidence: &Evidence) -> &'static Profile {
    evidence
        .configured
        .or_else(|| from_layout(evidence))
        .or_else(|| from_references(evidence))
        .unwrap_or_else(generic)
}

/// A package folder named after the MI (`.../ZZMI/d3dx.ini`), or one holding `Core/<MI>/`.
/// Only profiles with a library match: a folder called `XXMI` says nothing about the game.
fn from_layout(evidence: &Evidence) -> Option<&'static Profile> {
    let root = evidence.root?;
    let named = root
        .file_name()
        .and_then(|name| by_id(&name.to_string_lossy()))
        .filter(|profile| profile.snapshot.is_some());
    named.or_else(|| core_library(root, evidence.resolver))
}

fn core_library(root: &Path, resolver: &PathResolver) -> Option<&'static Profile> {
    let core = child_named(root, "core", resolver)?;
    PROFILES
        .iter()
        .filter(|p| p.snapshot.is_some())
        .find(|p| child_named(&core, p.id, resolver).is_some())
}

/// The path of the entry of `dir` called `name`, in any letter case.
fn child_named(dir: &Path, name: &str, resolver: &PathResolver) -> Option<std::path::PathBuf> {
    let names = resolver.list(dir)?;
    let found = names.iter().find(|n| n.eq_ignore_ascii_case(name))?;
    Some(dir.join(found))
}

/// The one profile whose library owns every namespace the package's references name, if the
/// references point at exactly one.
fn from_references(evidence: &Evidence) -> Option<&'static Profile> {
    let owners = namespace_owners(evidence.snapshots);
    let mut found: HashSet<&str> = HashSet::new();

    for file in evidence.files {
        for reference in &file.references {
            let Some(namespace) = library_namespace(&reference.text) else {
                continue;
            };
            if let Some([owner]) = owners.get(&namespace.to_lowercase()).map(Vec::as_slice) {
                found.insert(owner);
            }
        }
    }

    let mut found = found.into_iter();
    match (found.next(), found.next()) {
        (Some(id), None) => by_id(id),
        _ => None,
    }
}

/// Snapshot names by lower-cased namespace.
fn namespace_owners(snapshots: &[SymbolSnapshot]) -> HashMap<String, Vec<&str>> {
    let mut owners: HashMap<String, Vec<&str>> = HashMap::new();
    for snapshot in snapshots {
        for namespace in &snapshot.namespaces {
            let list = owners.entry(namespace.to_lowercase()).or_default();
            if !list.contains(&snapshot.name.as_str()) {
                list.push(&snapshot.name);
            }
        }
    }
    owners
}

/// `ns` of `Prefix\ns\Name` and `$\ns\name`: a reference that names a namespace.
fn library_namespace(text: &str) -> Option<&str> {
    let mut parts = text.split('\\');
    let (_, namespace, _) = (parts.next()?, parts.next()?, parts.next()?);
    Some(namespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_is_the_middle_part_of_a_qualified_reference() {
        assert_eq!(library_namespace("CommandList\\WWMI\\Run"), Some("WWMI"));
        assert_eq!(library_namespace("$\\srmi\\x"), Some("srmi"));
        assert_eq!(library_namespace("$local"), None);
        assert_eq!(library_namespace("Resource\\Foo"), None);
    }
}
