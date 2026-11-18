//! `xxmi`: see docs/04-cli.md. Exit codes: 0 ok, 1 lint errors (or --max-warnings exceeded),
//! 2 usage error, 3 internal error. Run as `xxmi-lsp` (or with `--stdio`) it is the language server.

mod args;
mod error;
mod help;
mod index_command;
mod js_number;
mod lint_command;
mod lsp;
mod lsp_command;
mod query_command;

use std::process::exit;

use error::CliError;
use help::USAGE;

fn main() {
    install_panic_hook();

    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    exit(report(dispatch(&args)));
}

/// The language server is the whole program when it is launched under its own name.
fn is_standalone_lsp() -> bool {
    let program = std::env::args_os().next().map(std::path::PathBuf::from);
    let stem = program.as_deref().and_then(|p| p.file_stem());
    stem.is_some_and(|stem| stem.eq_ignore_ascii_case("xxmi-lsp"))
}

/// The release profile aborts on panic, which would exit with a signal; exit 3 like the
/// TypeScript CLI does for an internal error.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("xxmi: internal error (please report it): {info}");
        exit(3);
    }));
}

fn dispatch(args: &[String]) -> Result<i32, CliError> {
    if is_standalone_lsp() {
        return lsp_command::run(args, true);
    }
    let Some((command, rest)) = args.split_first() else {
        println!("{USAGE}");
        return Ok(2);
    };

    match command.as_str() {
        "lint" => lint_command::run(rest),
        "query" => query_command::run(rest),
        "index" => index_command::run(rest),
        "lsp" => lsp_command::run(rest, false),
        "--stdio" => lsp_command::run(args, false),
        "--version" | "-v" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => Err(CliError::Usage(format!("unknown command {other}"))),
    }
}

fn report(outcome: Result<i32, CliError>) -> i32 {
    match outcome {
        Ok(code) => code,
        Err(CliError::Usage(message)) => {
            eprintln!("xxmi: {message}\n\n{USAGE}");
            2
        }
        Err(CliError::Internal(message)) => {
            eprintln!("xxmi: internal error (please report it): {message}");
            3
        }
    }
}
