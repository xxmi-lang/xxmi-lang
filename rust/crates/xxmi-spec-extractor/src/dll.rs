//! Loading a 3DMigoto-family DLL checkout and running extraction steps against it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use crate::cpp::{AnchorNotFound, CppFile};
use crate::model::GitInfo;

const COMPONENT_DIRS: [&str; 2] = ["D3DCompiler", "Injector"];

/// Why extraction stopped.
#[derive(Debug)]
pub enum ExtractError {
    Anchor(AnchorNotFound),
    Io(String),
}

impl From<AnchorNotFound> for ExtractError {
    fn from(error: AnchorNotFound) -> Self {
        Self::Anchor(error)
    }
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anchor(error) => error.fmt(f),
            Self::Io(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ExtractError {}

fn io_error(path: &Path, error: io::Error) -> ExtractError {
    ExtractError::Io(format!("{}: {error}", path.display()))
}

/// The DirectX11 sources of a 3DMigoto-family DLL checkout.
pub struct DllSource {
    files: BTreeMap<String, CppFile>,
    /// Component folders (`D3DCompiler`, `Injector`) the checkout doesn't have.
    pub missing_components: Vec<String>,
}

impl DllSource {
    pub fn new(files: impl IntoIterator<Item = CppFile>, missing_components: Vec<String>) -> Self {
        Self {
            files: files.into_iter().map(|f| (f.name.clone(), f)).collect(),
            missing_components,
        }
    }

    /// Loads `<root>/DirectX11/*.{cpp,h}` plus the shared `<root>/util.h` (DXGI format names).
    pub fn load(root: &Path) -> Result<Self, ExtractError> {
        Self::load_with(root, &|_, text| text)
    }

    /// As `load`, passing each file's text through `edit` first (tests simulate source drift).
    pub fn load_with(
        root: &Path,
        edit: &dyn Fn(&str, String) -> String,
    ) -> Result<Self, ExtractError> {
        let mut files = Vec::new();
        for name in sorted_names(&root.join("DirectX11"), |n| {
            n.ends_with(".cpp") || n.ends_with(".h")
        })? {
            files.push(read_source(
                &root.join("DirectX11").join(&name),
                name,
                edit,
            )?);
        }
        let util = root.join("util.h");
        if util.exists() {
            files.push(read_source(&util, "util.h".to_string(), edit)?);
        }

        // Other binaries built from the same repo read d3dx.ini too: the D3DCompiler wrapper and
        // the Injector (3DMigoto Loader). Only their literal-section reads are used.
        let mut missing = Vec::new();
        for component in COMPONENT_DIRS {
            let dir = root.join(component);
            if !dir.exists() {
                missing.push(component.to_string());
                continue;
            }
            for name in sorted_names(&dir, |n| n.ends_with(".cpp"))? {
                files.push(read_source(
                    &dir.join(&name),
                    format!("{component}/{name}"),
                    edit,
                )?);
            }
        }
        Ok(Self::new(files, missing))
    }

    pub fn file(&self, name: &str) -> Result<&CppFile, AnchorNotFound> {
        self.files
            .get(name)
            .ok_or_else(|| AnchorNotFound::new(name, "file", ""))
    }

    pub fn into_files(self) -> Vec<CppFile> {
        self.files.into_values().collect()
    }

    /// All files in name order, for scans that aren't tied to one symbol.
    pub fn all(&self) -> impl Iterator<Item = &CppFile> {
        self.files.values()
    }
}

/// Entry names of `dir` passing `keep`, in UTF-16 order like JavaScript's default sort.
fn sorted_names(dir: &Path, keep: impl Fn(&str) -> bool) -> Result<Vec<String>, ExtractError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| io_error(dir, e))? {
        let name = entry.map_err(|e| io_error(dir, e))?.file_name();
        let name = name.to_string_lossy().into_owned();
        if keep(&name) {
            names.push(name);
        }
    }
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    Ok(names)
}

fn read_source(
    path: &Path,
    name: String,
    edit: &dyn Fn(&str, String) -> String,
) -> Result<CppFile, ExtractError> {
    let bytes = fs::read(path).map_err(|e| io_error(path, e))?;
    let text = edit(&name, String::from_utf8_lossy(&bytes).into_owned());
    Ok(CppFile::new(name, &text))
}

/// Runs extraction steps. In strict mode (the XXMI DLL) a missing anchor aborts extraction, so
/// source drift is noticed instead of silently shrinking the spec. In lenient mode (the vanilla
/// baseline, which lacks XXMI-only tables) a missing anchor is recorded and the step yields its
/// fallback. An `xxmi_only` step is expected to miss in the baseline, so it falls back silently.
pub struct Extraction {
    pub dll: DllSource,
    pub strict: bool,
    warnings: RefCell<Vec<String>>,
}

impl Extraction {
    pub fn new(dll: DllSource, strict: bool) -> Self {
        Self {
            dll,
            strict,
            warnings: RefCell::new(Vec::new()),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings.borrow().clone()
    }

    pub fn step<T>(
        &self,
        fallback: T,
        run: impl FnOnce() -> Result<T, AnchorNotFound>,
    ) -> Result<T, AnchorNotFound> {
        self.step_with(fallback, run, false)
    }

    pub fn step_xxmi_only<T>(
        &self,
        fallback: T,
        run: impl FnOnce() -> Result<T, AnchorNotFound>,
    ) -> Result<T, AnchorNotFound> {
        self.step_with(fallback, run, true)
    }

    fn step_with<T>(
        &self,
        fallback: T,
        run: impl FnOnce() -> Result<T, AnchorNotFound>,
        xxmi_only: bool,
    ) -> Result<T, AnchorNotFound> {
        match run() {
            Err(error) if self.strict => Err(error),
            Err(error) => {
                if !xxmi_only {
                    self.warnings.borrow_mut().push(error.to_string());
                }
                Ok(fallback)
            }
            Ok(value) => Ok(value),
        }
    }
}

/// Commit identity of a git checkout; fields stay empty when git can't say.
pub fn git_info(root: &Path) -> Result<GitInfo, ExtractError> {
    let log = git(root, &["log", "-1", "--format=%H%n%cI"])?;
    let mut lines = log.split('\n');
    let commit = lines.next().unwrap_or("").to_string();
    let commit_date = lines.next().unwrap_or("").to_string();

    // A checkout without an origin remote (e.g. a local copy) has no repository URL.
    let repository = git(root, &["remote", "get-url", "origin"])
        .map(|url| {
            let url = url.strip_suffix(".git").unwrap_or(&url);
            match url.strip_prefix("git@github.com:") {
                Some(rest) => format!("https://github.com/{rest}"),
                None => url.to_string(),
            }
        })
        .unwrap_or_default();
    Ok(GitInfo {
        repository,
        commit,
        commit_date,
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String, ExtractError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| ExtractError::Io(format!("git: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ExtractError::Io(format!(
            "git {}: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
