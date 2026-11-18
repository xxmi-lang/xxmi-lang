//! Argument parsing with the behaviour of Node's strict `util.parseArgs`, so the Rust and
//! TypeScript CLIs accept and reject the same command lines.

use std::collections::{HashMap, HashSet};

use crate::error::CliError;

pub struct OptionSpec {
    pub name: &'static str,
    pub takes_value: bool,
}

pub const fn flag(name: &'static str) -> OptionSpec {
    OptionSpec {
        name,
        takes_value: false,
    }
}

pub const fn string(name: &'static str) -> OptionSpec {
    OptionSpec {
        name,
        takes_value: true,
    }
}

#[derive(Default)]
pub struct Parsed {
    strings: HashMap<String, String>,
    flags: HashSet<String>,
    pub positionals: Vec<String>,
}

impl Parsed {
    pub fn string(&self, name: &str) -> Option<&str> {
        self.strings.get(name).map(String::as_str)
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }
}

pub fn parse(args: &[String], specs: &[OptionSpec]) -> Result<Parsed, CliError> {
    let mut parsed = Parsed::default();
    let mut rest = args.iter().peekable();

    while let Some(arg) = rest.next() {
        if arg == "--" {
            parsed.positionals.extend(rest.by_ref().cloned());
            break;
        }
        let Some(long) = arg.strip_prefix("--") else {
            if arg.len() > 1 && arg.starts_with('-') {
                let short: String = arg.chars().take(2).collect();
                return Err(unknown_option(&short));
            }
            parsed.positionals.push(arg.clone());
            continue;
        };

        let (name, inline) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (long, None),
        };
        let Some(spec) = specs.iter().find(|s| s.name == name) else {
            return Err(unknown_option(&format!("--{name}")));
        };

        if !spec.takes_value {
            if inline.is_some() {
                return Err(CliError::Usage(format!(
                    "Option '--{name}' does not take an argument"
                )));
            }
            parsed.flags.insert(name.to_string());
            continue;
        }

        let value = match inline {
            Some(value) => value,
            None => take_value(name, rest.next())?,
        };
        parsed.strings.insert(name.to_string(), value);
    }
    Ok(parsed)
}

fn take_value(name: &str, next: Option<&String>) -> Result<String, CliError> {
    match next {
        None => Err(CliError::Usage(format!(
            "Option '--{name} <value>' argument missing"
        ))),
        Some(value) if value.len() > 1 && value.starts_with('-') => Err(CliError::Usage(format!(
            "Option '--{name}' argument is ambiguous.\nDid you forget to specify the option argument for '--{name}'?\nTo specify an option argument starting with a dash use '--{name}=-XYZ'."
        ))),
        Some(value) => Ok(value.clone()),
    }
}

fn unknown_option(option: &str) -> CliError {
    CliError::Usage(format!(
        "Unknown option '{option}'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"{option}\""
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPECS: [OptionSpec; 2] = [string("format"), flag("stdin")];

    fn parse_all(args: &[&str]) -> Result<Parsed, CliError> {
        let owned: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse(&owned, &SPECS)
    }

    #[test]
    fn options_and_positionals_mix() {
        let parsed = parse_all(&["a", "--stdin", "--format=json", "-", "b"]).unwrap();
        assert!(parsed.flag("stdin"));
        assert_eq!(parsed.string("format"), Some("json"));
        assert_eq!(parsed.positionals, ["a", "-", "b"]);
    }

    #[test]
    fn double_dash_ends_options() {
        let parsed = parse_all(&["--", "--a", "b"]).unwrap();
        assert_eq!(parsed.positionals, ["--a", "b"]);
    }

    #[test]
    fn rejects_what_node_rejects() {
        for args in [
            &["-x"][..],
            &["--zz"],
            &["--stdin=1"],
            &["--format"],
            &["--format", "--stdin"],
        ] {
            assert!(parse_all(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn empty_inline_value_is_a_value() {
        assert_eq!(
            parse_all(&["--format="]).unwrap().string("format"),
            Some("")
        );
    }
}
