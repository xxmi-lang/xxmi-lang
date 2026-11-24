//! One symbol scope per package: a mod never resolves against the files of another package,
//! and each package is checked against the library of its own profile.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::IniFile;
use crate::profile::{Evidence, Profile, detect};

use super::symbols::SymbolTable;
use super::{FileId, Workspace};

/// A value for each package root, and one for the files that sit in no package.
pub struct PerPackage<T> {
    implicit: T,
    packages: HashMap<PathBuf, T>,
}

impl<T> PerPackage<T> {
    pub fn get(&self, root: Option<&Path>) -> &T {
        root.and_then(|root| self.packages.get(root))
            .unwrap_or(&self.implicit)
    }

    pub fn map<U>(&self, mut f: impl FnMut(&T) -> U) -> PerPackage<U> {
        PerPackage {
            implicit: f(&self.implicit),
            packages: self
                .packages
                .iter()
                .map(|(k, v)| (k.clone(), f(v)))
                .collect(),
        }
    }
}

pub struct Scope {
    pub profile: &'static Profile,
    pub symbols: SymbolTable,
    pub files: Vec<FileId>,
}

pub type Scopes = PerPackage<Scope>;

pub(super) fn build(workspace: &Workspace) -> Scopes {
    let mut implicit = Vec::new();
    let mut packages: HashMap<&Path, Vec<FileId>> = HashMap::new();

    for (id, file) in workspace.files.iter().enumerate() {
        match workspace.package_roots.get(&file.path) {
            Some(root) => packages.entry(root).or_default().push(id),
            None => implicit.push(id),
        }
    }

    Scopes {
        implicit: scope(workspace, None, implicit),
        packages: packages
            .into_iter()
            .map(|(root, ids)| (root.to_path_buf(), scope(workspace, Some(root), ids)))
            .collect(),
    }
}

fn scope(workspace: &Workspace, root: Option<&Path>, files: Vec<FileId>) -> Scope {
    let profile = profile_of(workspace, root, &files);

    let mut symbols = SymbolTable::new(workspace.lookup);
    for &id in &files {
        symbols.add_file(id, &workspace.files[id]);
    }

    // A package that ships its own copy of the library wins over the snapshot.
    let library = workspace.library_of(profile);
    if let Some(snapshot) =
        library.filter(|s| !s.namespaces.iter().any(|ns| symbols.has_namespace(ns)))
    {
        symbols.add_snapshot(snapshot);
    }

    Scope {
        profile,
        symbols,
        files,
    }
}

fn profile_of(workspace: &Workspace, root: Option<&Path>, ids: &[FileId]) -> &'static Profile {
    if let Some(forced) = workspace.profile_override {
        return forced;
    }

    let files: Vec<&IniFile> = ids.iter().map(|&id| &workspace.files[id]).collect();
    detect(&Evidence {
        root,
        resolver: &workspace.resolver,
        configured: configured_profile(workspace, root, &files),
        files: &files,
        snapshots: workspace.snapshots.as_slice(),
    })
}

/// The `profile` of the `xxmi.toml` at or above the package root, or above the first file when
/// there is no package. A broken `xxmi.toml` is reported by the lint run, not here.
fn configured_profile(
    workspace: &Workspace,
    root: Option<&Path>,
    files: &[&IniFile],
) -> Option<&'static Profile> {
    let dir = root.or_else(|| files.first()?.path.parent())?;
    workspace.configs.for_directory(dir).ok()?.profile
}
