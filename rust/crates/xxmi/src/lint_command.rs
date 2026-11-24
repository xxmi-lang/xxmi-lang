//! `xxmi lint`: flags, validation order, output and exit codes.

use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};

use xxmi_core::encoding::decode_text;
use xxmi_core::lint::format::{format_json, format_text};
use xxmi_core::lint::{LintOptions, LintResult, lint_paths, lint_text};
use xxmi_core::lookup::Lookup;
use xxmi_core::paths::display_path;
use xxmi_core::profile::{self, Profile};
use xxmi_core::rules::RULES;
use xxmi_core::spec::Spec;

use crate::args::{self, OptionSpec};
use crate::error::CliError;
use crate::help::lint_help;
use crate::js_number::js_number;

const OPTIONS: [OptionSpec; 9] = [
    args::string("format"),
    args::string("rules"),
    args::string("max-warnings"),
    args::flag("stdin"),
    args::flag("require-package"),
    args::string("mi-package"),
    args::string("stdin-path"),
    args::string("spec"),
    args::flag("help"),
];

#[derive(PartialEq)]
enum Format {
    Text,
    Json,
}

/// Returns the process exit code.
pub fn run(raw_args: &[String]) -> Result<i32, CliError> {
    let parsed = args::parse(raw_args, &OPTIONS)?;
    if parsed.flag("help") {
        println!("{}", lint_help());
        return Ok(0);
    }

    let format = parse_format(parsed.string("format").unwrap_or("text"))?;
    let rules = parse_rules(parsed.string("rules"))?;
    let max_warnings = parse_max_warnings(parsed.string("max-warnings"))?;

    let mut options = LintOptions::bundled();
    options.rules = rules;
    options.require_package = parsed.flag("require-package");
    options.profile = parse_mi_package(parsed.string("mi-package"))?;
    if let Some(path) = parsed.string("spec") {
        options.lookup = load_spec(Path::new(path))?;
    }

    let result = if parsed.flag("stdin") {
        let Some(path) = parsed.string("stdin-path").filter(|p| !p.is_empty()) else {
            return Err(usage("--stdin needs --stdin-path <path>"));
        };
        if !parsed.positionals.is_empty() {
            return Err(usage("--stdin lints stdin only; drop the paths"));
        }
        lint_stdin(Path::new(path), &options)?
    } else {
        if parsed.string("stdin-path").is_some() {
            return Err(usage("--stdin-path needs --stdin"));
        }
        lint_disk(&parsed.positionals, &options)?
    };

    print_result(&result, &format)?;
    Ok(exit_code(&result, max_warnings))
}

fn usage(message: &str) -> CliError {
    CliError::Usage(message.to_string())
}

fn parse_format(value: &str) -> Result<Format, CliError> {
    match value {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        other => Err(CliError::Usage(format!("unknown --format {other}"))),
    }
}

fn parse_rules(value: Option<&str>) -> Result<Option<Vec<String>>, CliError> {
    let Some(value) = value else {
        return Ok(None);
    };

    let rules: Vec<String> = value
        .split(',')
        .map(|r| r.trim().to_uppercase())
        .filter(|r| !r.is_empty())
        .collect();
    for rule in &rules {
        if !RULES.iter().any(|known| known.id == rule) {
            return Err(CliError::Usage(format!("unknown rule {rule}")));
        }
    }
    Ok(Some(rules))
}

fn parse_mi_package(value: Option<&str>) -> Result<Option<&'static Profile>, CliError> {
    let Some(value) = value else {
        return Ok(None);
    };

    profile::by_id(value).map(Some).ok_or_else(|| {
        let known: Vec<&str> = profile::ids().collect();
        CliError::Usage(format!(
            "unknown --mi-package {value}; use one of {}",
            known.join(", ")
        ))
    })
}

fn parse_max_warnings(value: Option<&str>) -> Result<Option<f64>, CliError> {
    let Some(value) = value else {
        return Ok(None);
    };

    let number = js_number(value);
    if number.is_finite() && number.fract() == 0.0 && number >= 0.0 {
        Ok(Some(number))
    } else {
        Err(usage("--max-warnings must be a non-negative integer"))
    }
}

/// A runtime spec replaces the built-in one; leaked because the lookup lives for the whole run.
fn load_spec(path: &Path) -> Result<&'static Lookup, CliError> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| CliError::Usage(format!("cannot read --spec {}: {e}", path.display())))?;
    let spec = Spec::from_json(&json)
        .map_err(|e| CliError::Usage(format!("invalid --spec {}: {e}", path.display())))?;
    Ok(Box::leak(Box::new(Lookup::new(spec.leak()))))
}

fn lint_stdin(path: &Path, options: &LintOptions) -> Result<LintResult, CliError> {
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;

    let (text, encoding) = decode_text(&bytes);
    Ok(lint_text(path, text, encoding, options)?)
}

fn lint_disk(positionals: &[String], options: &LintOptions) -> Result<LintResult, CliError> {
    let paths: Vec<PathBuf> = if positionals.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        positionals.iter().map(PathBuf::from).collect()
    };

    for path in &paths {
        if std::fs::metadata(path).is_err() {
            return Err(CliError::Usage(format!(
                "no such file or directory: {}",
                path.display()
            )));
        }
    }
    Ok(lint_paths(&paths, options)?)
}

fn print_result(result: &LintResult, format: &Format) -> Result<(), CliError> {
    let cwd = env::current_dir().unwrap_or_default();
    let path_of = |path: &Path| display_path(path, &cwd);

    let output = if *format == Format::Json {
        format_json(result, &path_of).map_err(|error| CliError::Internal(error.to_string()))?
    } else {
        format_text(result, &path_of)
    };
    println!("{output}");
    Ok(())
}

fn exit_code(result: &LintResult, max_warnings: Option<f64>) -> i32 {
    let too_many_warnings = max_warnings.is_some_and(|max| result.summary.warning as f64 > max);
    i32::from(result.summary.error > 0 || too_many_warnings)
}
