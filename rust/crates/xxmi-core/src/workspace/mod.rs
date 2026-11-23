//! Workspace index (port of packages/core/src/workspace/workspace.ts, docs/01-language-model.md §4):
//! loads files with the namespaces the DLL would give them, follows `[Include]` like
//! `ParseIncludedIniFiles`, and builds the symbol table.

pub mod load_order;
pub mod package;
pub mod snapshot;
pub mod symbols;
pub mod usage;

use std::cell::{OnceCell, RefCell};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::document::Document;
use crate::encoding::{Encoding, decode_text};
use crate::glob::Glob;
use crate::lookup::Lookup;
use crate::model::{IniFile, LowerOptions, lower_ini};
use crate::paths::{PathResolver, relative_path, resolve_path};
use crate::trace;

use package::{default_excludes, find_package_root, path_namespace};
use snapshot::Snapshots;
use symbols::SymbolTable;
use usage::Usage;

pub type FileId = usize;

pub struct Workspace {
    lookup: &'static Lookup,
    resolver: PathResolver,
    snapshots: Snapshots,
    /// In load order, which decides which of two duplicate sections the DLL keeps.
    files: Vec<IniFile>,
    file_ids: HashMap<PathBuf, FileId>,
    /// Package root each file was loaded under, if any.
    package_roots: HashMap<PathBuf, PathBuf>,
    /// Where each file's namespace is counted from.
    namespace_roots: HashMap<PathBuf, PathBuf>,
    /// Files that were linted or explicitly added, as opposed to pulled in by includes.
    targets: HashSet<PathBuf>,
    loaded_packages: HashSet<PathBuf>,
    /// `exclude_recursive` globs from each loaded package's d3dx.ini `[Include]`.
    excludes: HashMap<PathBuf, Vec<Glob>>,
    default_excludes: Vec<Glob>,
    /// Files reached through `include =` (or d3dx.ini itself), which exclusions don't apply to.
    included: HashSet<PathBuf>,
    /// Namespaces that were given rather than derived from the path (d3dx.ini's ``), so a reload
    /// lowers the file the same way.
    fixed_namespaces: HashMap<PathBuf, String>,
    /// Text of files open in an editor; wins over the file on disk.
    overlays: HashMap<PathBuf, String>,
    /// Parse state of the files that have an overlay.
    documents: HashMap<PathBuf, Document>,
    /// `canonicalize` results: symlinked libraries share an editor buffer with their target.
    real_paths: RefCell<HashMap<PathBuf, PathBuf>>,
    symbols: OnceCell<SymbolTable>,
    usage: OnceCell<Usage>,
}

impl Workspace {
    pub fn new(lookup: &'static Lookup, snapshots: impl Into<Snapshots>) -> Self {
        Self::with_resolver(lookup, snapshots, PathResolver::default())
    }

    pub fn with_resolver(
        lookup: &'static Lookup,
        snapshots: impl Into<Snapshots>,
        resolver: PathResolver,
    ) -> Self {
        Self {
            lookup,
            resolver,
            snapshots: snapshots.into(),
            files: Vec::new(),
            file_ids: HashMap::new(),
            package_roots: HashMap::new(),
            namespace_roots: HashMap::new(),
            targets: HashSet::new(),
            loaded_packages: HashSet::new(),
            excludes: HashMap::new(),
            default_excludes: default_excludes(),
            included: HashSet::new(),
            fixed_namespaces: HashMap::new(),
            overlays: HashMap::new(),
            documents: HashMap::new(),
            real_paths: RefCell::new(HashMap::new()),
            symbols: OnceCell::new(),
            usage: OnceCell::new(),
        }
    }

    pub fn lookup(&self) -> &'static Lookup {
        self.lookup
    }

    pub fn resolver(&self) -> &PathResolver {
        &self.resolver
    }

    pub fn files(&self) -> &[IniFile] {
        &self.files
    }

    pub fn file_id(&self, path: &Path) -> Option<FileId> {
        self.file_ids.get(path).copied()
    }

    pub fn file_mut(&mut self, id: FileId) -> &mut IniFile {
        &mut self.files[id]
    }

    pub fn is_target(&self, file: &IniFile) -> bool {
        self.targets.contains(&file.path)
    }

    pub fn package_root(&self, file: &IniFile) -> Option<&Path> {
        self.package_roots.get(&file.path).map(PathBuf::as_path)
    }

    /// Registers editor-buffer text that wins over the file on disk (the CLI's `--stdin`).
    pub fn set_overlay(&mut self, path: &Path, text: String) {
        self.overlays.insert(resolve_path(path), text);
    }

    /// Opens (or updates) a file from an editor buffer: the text is used instead of the disk
    /// contents until `close_document`. Re-lowers the file and follows any new includes.
    pub fn open_document(
        &mut self,
        path: &Path,
        text: String,
        fallback_root: &Path,
    ) -> io::Result<FileId> {
        let full = resolve_path(path);
        let aliases = self.aliases(&full);
        for alias in &aliases {
            self.overlays.insert(alias.clone(), text.clone());
        }
        self.overlays.insert(full.clone(), text);

        for alias in &aliases {
            self.reload(alias);
        }
        if self.file_ids.contains_key(&full)
            && let Some(id) = self.reload(&full)
        {
            self.targets.insert(full);
            return Ok(id);
        }
        self.add_target(&full, fallback_root)
    }

    /// Drops the editor buffer; the file is re-read from disk (or removed if it's gone).
    pub fn close_document(&mut self, path: &Path) {
        let full = resolve_path(path);
        let mut paths = self.aliases(&full);
        paths.insert(0, full);

        for path in paths {
            self.overlays.remove(&path);
            self.documents.remove(&path);
            self.reload(&path);
        }
    }

    /// Re-lowers a loaded file from its overlay or from disk (after a file-watcher event). A file
    /// that no longer exists is removed. The file moves to the end of the load order, which is
    /// what decides between duplicate sections, so ids from before the call are stale.
    pub fn reload(&mut self, path: &Path) -> Option<FileId> {
        let full = resolve_path(path);
        let id = self.file_ids.get(&full).copied()?;
        let namespace_root = self.namespace_roots.get(&full)?.clone();
        let package_root = self.package_roots.get(&full).cloned();
        let namespace = self.fixed_namespaces.get(&full).cloned();

        self.remove_file(id, &full);
        match self.load(&full, &namespace_root, package_root.as_deref(), namespace) {
            Ok(id) => {
                // A missing include only costs references; the file itself is fine.
                let _ = self.follow_includes(id, &namespace_root, package_root.as_deref());
                Some(id)
            }
            Err(_) => {
                self.namespace_roots.remove(&full);
                self.package_roots.remove(&full);
                self.fixed_namespaces.remove(&full);
                self.targets.remove(&full);
                None
            }
        }
    }

    /// Other loaded paths of the same file on disk (symlinks: a library linked into `Core/` while
    /// its working copy lives elsewhere). They follow the editor buffer of any of them.
    fn aliases(&self, path: &Path) -> Vec<PathBuf> {
        let real = self.real_path(path);
        self.files
            .iter()
            .map(|file| &file.path)
            .filter(|other| other.as_path() != path && self.real_path(other) == real)
            .cloned()
            .collect()
    }

    fn real_path(&self, path: &Path) -> PathBuf {
        if let Some(real) = self.real_paths.borrow().get(path) {
            return real.clone();
        }
        let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.real_paths
            .borrow_mut()
            .insert(path.to_path_buf(), real.clone());
        real
    }

    fn remove_file(&mut self, id: FileId, path: &Path) {
        self.files.remove(id);
        self.file_ids.remove(path);
        for other in self.file_ids.values_mut().filter(|other| **other > id) {
            *other -= 1;
        }
        self.symbols = OnceCell::new();
        self.usage = OnceCell::new();
    }

    /// The symbols of every loaded file, plus the snapshots of libraries no loaded file provides.
    pub fn symbols(&self) -> &SymbolTable {
        self.symbols.get_or_init(|| {
            trace::span("symbols", || {
                let mut table = SymbolTable::new(self.lookup);
                for (id, file) in self.files.iter().enumerate() {
                    table.add_file(id, file);
                }
                for snapshot in self.snapshots.as_slice() {
                    if !snapshot.namespaces.iter().any(|ns| table.has_namespace(ns)) {
                        table.add_snapshot(snapshot);
                    }
                }
                table
            })
        })
    }

    pub fn usage(&self) -> &Usage {
        self.usage
            .get_or_init(|| trace::span("usage", || usage::compute(self)))
    }

    /// Adds a file to lint. Its namespace comes from its path relative to the package root (or to
    /// `fallback_root` when there is none, which keeps namespaces unique within the run).
    pub fn add_target(&mut self, path: &Path, fallback_root: &Path) -> io::Result<FileId> {
        let full = resolve_path(path);
        let root = full
            .parent()
            .and_then(|dir| find_package_root(dir, &self.resolver));
        if let Some(root) = &root {
            self.load_package(root)?;
        }

        let namespace_root = root.as_deref().unwrap_or(fallback_root);
        let id = self.load(&full, namespace_root, root.as_deref(), None)?;
        self.targets.insert(full);
        self.follow_includes(id, namespace_root, root.as_deref())?;
        Ok(id)
    }

    /// Loads d3dx.ini and everything it `include`s (not `include_recursive` folders).
    pub fn load_package(&mut self, root: &Path) -> io::Result<()> {
        if !self.loaded_packages.insert(root.to_path_buf()) {
            return Ok(());
        }
        let Some(d3dx) = self.resolver.list(root).and_then(|names| {
            names
                .iter()
                .find(|n| n.to_lowercase() == "d3dx.ini")
                .cloned()
        }) else {
            return Ok(());
        };

        let id = self.load(&root.join(d3dx), root, Some(root), Some(String::new()))?;
        self.included.insert(self.files[id].path.clone());
        self.excludes
            .insert(root.to_path_buf(), exclude_globs(&self.files[id]));
        self.follow_includes(id, root, Some(root))
    }

    /// Whether the DLL skips `path` while walking `include_recursive` folders: a folder or file
    /// name between `base` (default: the package root) and the file matches the package's
    /// `exclude_recursive` globs (`DISABLED*` outside a package). Files reached through
    /// `include =` are never excluded. Loads the package if needed.
    pub fn excluded(&mut self, path: &Path, base: Option<&Path>) -> io::Result<bool> {
        let full = resolve_path(path);
        let root = self.package_roots.get(&full).cloned().or_else(|| {
            full.parent()
                .and_then(|dir| find_package_root(dir, &self.resolver))
        });
        if let Some(root) = &root {
            self.load_package(root)?;
        }
        Ok(self.is_excluded(&full, base))
    }

    /// `excluded` for a file whose package is already loaded.
    pub fn is_excluded(&self, path: &Path, base: Option<&Path>) -> bool {
        let full = resolve_path(path);
        if self.included.contains(&full) {
            return false;
        }

        let root = self.package_roots.get(&full).cloned().or_else(|| {
            full.parent()
                .and_then(|dir| find_package_root(dir, &self.resolver))
        });
        let from = base
            .map(Path::to_path_buf)
            .or_else(|| root.clone())
            .or_else(|| self.namespace_roots.get(&full).cloned());
        let Some(from) = from else {
            return false;
        };

        let relative = relative_path(&from, &full);
        if relative.starts_with("..") || relative.is_empty() {
            return false;
        }

        let patterns = root
            .as_ref()
            .and_then(|r| self.excludes.get(r))
            .unwrap_or(&self.default_excludes);
        relative
            .split(std::path::MAIN_SEPARATOR)
            .any(|name| patterns.iter().any(|p| p.is_match(name)))
    }

    fn load(
        &mut self,
        path: &Path,
        namespace_root: &Path,
        package_root: Option<&Path>,
        fixed_namespace: Option<String>,
    ) -> io::Result<FileId> {
        if let Some(&id) = self.file_ids.get(path) {
            return Ok(id);
        }

        let (text, encoding) = self.read_text(path)?;
        let open = self.overlay_for(path).is_some();
        let lookup = self.lookup;
        let options = |document| LowerOptions {
            path: path.to_path_buf(),
            document,
            encoding,
            default_namespace: fixed_namespace
                .clone()
                .unwrap_or_else(|| path_namespace(path, namespace_root)),
            lookup,
        };
        let file = if open {
            let document = trace::span("parse", || self.document_for(path, text));
            trace::span("lower", || lower_ini(options(document)))
        } else {
            let document = trace::span("parse", || Document::new(text));
            trace::span("lower", || lower_ini(options(&document)))
        };

        // Anything cached was computed from the files loaded so far.
        self.symbols = OnceCell::new();
        self.usage = OnceCell::new();

        let id = self.files.len();
        self.files.push(file);
        self.file_ids.insert(path.to_path_buf(), id);
        self.namespace_roots
            .insert(path.to_path_buf(), namespace_root.to_path_buf());
        if let Some(root) = package_root {
            self.package_roots
                .insert(path.to_path_buf(), root.to_path_buf());
        }
        if let Some(namespace) = fixed_namespace {
            self.fixed_namespaces.insert(path.to_path_buf(), namespace);
        }
        Ok(id)
    }

    /// The parsed text of an open file. Its trees outlive the edit, so the next edit only parses
    /// the sections that changed.
    fn document_for(&mut self, path: &Path, text: String) -> &Document {
        match self.documents.entry(path.to_path_buf()) {
            Entry::Occupied(entry) => {
                let document = entry.into_mut();
                document.set_text(text);
                document
            }
            Entry::Vacant(entry) => entry.insert(Document::new(text)),
        }
    }

    /// The overlay for `path` (or for another path to the same file on disk), else the file.
    fn read_text(&self, path: &Path) -> io::Result<(String, Encoding)> {
        match self.overlay_for(path) {
            Some(text) => Ok((text.clone(), Encoding::Utf8)),
            None => Ok(decode_text(&fs::read(path)?)),
        }
    }

    fn overlay_for(&self, path: &Path) -> Option<&String> {
        if let Some(text) = self.overlays.get(path) {
            return Some(text);
        }
        if self.overlays.is_empty() {
            return None;
        }

        let real = self.real_path(path);
        self.overlays
            .iter()
            .find_map(|(other, text)| (self.real_path(other) == real).then_some(text))
    }

    /// `include = ...` in `[Include...]` sections, relative to the including file's folder.
    fn follow_includes(
        &mut self,
        id: FileId,
        namespace_root: &Path,
        package_root: Option<&Path>,
    ) -> io::Result<()> {
        for found in self.include_targets(id) {
            self.included.insert(found.clone());
            if self.file_ids.contains_key(&found) {
                continue;
            }
            let included = self.load(&found, namespace_root, package_root, None)?;
            self.follow_includes(included, namespace_root, package_root)?;
        }
        Ok(())
    }

    /// Existing files named by the `include =` lines of `id`.
    fn include_targets(&self, id: FileId) -> Vec<PathBuf> {
        let file = &self.files[id];
        let dir = file.path.parent().unwrap_or(Path::new(""));

        let mut targets = Vec::new();
        for section in file
            .sections
            .iter()
            .filter(|s| s.spec.is_some_and(|s| s.name == "Include"))
        {
            for line in &section.lines {
                let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                    continue;
                };
                let value = file.slice(value);
                if file.slice(key).to_lowercase() != "include" || value.is_empty() {
                    continue;
                }
                if let Some(found) = self.resolver.resolve(dir, value)
                    && !self.resolver.is_directory(&found)
                {
                    targets.push(found);
                }
            }
        }
        targets
    }
}

/// The `exclude_recursive` globs of d3dx.ini's `[Include]` sections.
fn exclude_globs(main: &IniFile) -> Vec<Glob> {
    let mut globs = Vec::new();
    for section in main
        .sections
        .iter()
        .filter(|s| main.slice(s.name).to_lowercase() == "include")
    {
        for line in &section.lines {
            let (Some(key), Some(value)) = (line.scanned.key, line.scanned.value) else {
                continue;
            };
            let value = main.slice(value);
            if main.slice(key).to_lowercase() == "exclude_recursive" && !value.is_empty() {
                globs.push(Glob::new(value));
            }
        }
    }
    globs
}
