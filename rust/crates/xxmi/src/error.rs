//! What a command can fail with, and the exit code each kind maps to.

use std::fmt;

use xxmi_core::lint::LintError;

#[derive(Debug)]
pub enum CliError {
    /// Bad command line or a bad `xxmi.toml`: exit 2, with the usage text.
    Usage(String),
    /// Something that should not happen: exit 3.
    Internal(String),
}

impl From<LintError> for CliError {
    fn from(error: LintError) -> Self {
        match error {
            LintError::Config(error) => Self::Usage(error.to_string()),
            LintError::Io(error) => Self::Internal(error.to_string()),
            LintError::Cancelled => Self::Internal("lint cancelled".into()),
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::Internal(message) => f.write_str(message),
        }
    }
}
