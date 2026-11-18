//! `xxmi lsp [--stdio]` and the standalone `xxmi-lsp`: the language server on stdin/stdout.

use crate::error::CliError;
use crate::lsp;

const LSP_HELP: &str = "xxmi lsp [--stdio]

Starts the language server for XXMI / 3DMigoto mod .ini files on stdin/stdout, exactly like
`xxmi-lsp`. stdio is the only transport, so --stdio is optional.";

const STANDALONE_HELP: &str =
    "Usage: xxmi-lsp [--stdio]\n\nLanguage server for XXMI / 3DMigoto mod .ini files (stdio).";

/// `standalone` is the `xxmi-lsp` entry point, whose help text differs from `xxmi lsp`'s.
pub fn run(args: &[String], standalone: bool) -> Result<i32, CliError> {
    let has = |long: &str, short: &str| args.iter().any(|a| a == long || a == short);

    if has("--version", "-v") {
        println!("{}", env!("CARGO_PKG_VERSION"));
    } else if has("--help", "-h") {
        let help = if standalone {
            STANDALONE_HELP
        } else {
            LSP_HELP
        };
        println!("{help}");
    } else {
        lsp::run().map_err(|error| CliError::Internal(error.to_string()))?;
    }
    Ok(0)
}
