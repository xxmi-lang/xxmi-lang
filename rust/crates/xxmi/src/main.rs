//! `xxmi-rs`: the Rust spike of the `xxmi` CLI (`lint`) and `xxmi-lsp` (`lsp --stdio`).

mod lint_command;
mod lsp;
mod paths;

use std::process::ExitCode;

const USAGE: &str = "Usage:
  xxmi-rs lint <path>... [--format json] [--rules XM001,XM102]
  xxmi-rs lsp --stdio
  xxmi-rs --version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--version" | "-v") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("lint") => report(lint_command::run(&args[1..])),
        Some("lsp") => report(lsp::run().map(|()| ExitCode::SUCCESS)),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn report<E: std::fmt::Display>(result: Result<ExitCode, E>) -> ExitCode {
    result.unwrap_or_else(|error| {
        eprintln!("xxmi-rs: {error}");
        ExitCode::from(2)
    })
}
