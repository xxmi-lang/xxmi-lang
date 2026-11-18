//! The lint engine (port of packages/core/src/lint): runs the rules over a workspace and renders
//! the result. The CLI, and later the LSP and MCP frontends, lint through here.

mod collect;
pub mod config;
mod file;
pub mod format;
mod run;
mod suppress;

use std::fmt;
use std::io;
use std::path::PathBuf;

use serde::Serialize;

use crate::defaults::{bundled_lookup, bundled_snapshots};
use crate::diagnostic::Diagnostic;
use crate::encoding::Encoding;
use crate::lookup::Lookup;
use crate::workspace::snapshot::SymbolSnapshot;

pub use collect::collect_ini_files;
pub use config::ConfigError;
pub use run::{lint_paths, lint_text};

pub struct LintOptions {
    /// Only run these rule ids (upper-case).
    pub rules: Option<Vec<String>>,
    pub lookup: &'static Lookup,
    pub snapshots: &'static [SymbolSnapshot],
}

impl LintOptions {
    /// The bundled spec and library snapshots.
    pub fn bundled() -> Self {
        Self {
            rules: None,
            lookup: bundled_lookup(),
            snapshots: bundled_snapshots(),
        }
    }
}

pub struct FileResult {
    pub path: PathBuf,
    pub encoding: Encoding,
    pub diagnostics: Vec<Diagnostic>,
}

/// Counts in the key order of the JSON output.
#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub files: usize,
    pub error: usize,
    pub warning: usize,
    pub info: usize,
    pub hint: usize,
}

pub struct LintResult {
    pub files: Vec<FileResult>,
    pub summary: Summary,
}

#[derive(Debug)]
pub enum LintError {
    /// A bad `xxmi.toml`: a usage error, not a crash.
    Config(ConfigError),
    Io(io::Error),
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LintError {}

impl From<ConfigError> for LintError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<io::Error> for LintError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// `^XM\d{3}$`
fn is_rule_id(id: &str) -> bool {
    id.strip_prefix("XM")
        .is_some_and(|digits| digits.len() == 3 && digits.bytes().all(|b| b.is_ascii_digit()))
}
