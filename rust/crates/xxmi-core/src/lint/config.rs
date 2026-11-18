//! `xxmi.toml`: the nearest one up the tree from a file wins (docs/02-diagnostics.md
//! "Configuration").
//!
//!   [rules]
//!   XM204 = "off"

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use toml::{Table, Value};

use crate::diagnostic::Severity;

use super::is_rule_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSetting {
    Off,
    Level(Severity),
}

const SETTINGS: [(&str, RuleSetting); 5] = [
    ("off", RuleSetting::Off),
    ("error", RuleSetting::Level(Severity::Error)),
    ("warning", RuleSetting::Level(Severity::Warning)),
    ("info", RuleSetting::Level(Severity::Info)),
    ("hint", RuleSetting::Level(Severity::Hint)),
];

impl RuleSetting {
    /// `off`, `error`, `warning`, `info` or `hint`.
    pub fn parse(name: &str) -> Option<Self> {
        SETTINGS
            .iter()
            .find(|(setting, _)| *setting == name)
            .map(|(_, setting)| *setting)
    }
}

#[derive(Debug)]
pub struct ConfigError(String);

impl ConfigError {
    fn new(path: &Path, message: impl fmt::Display) -> Self {
        Self(format!("{}: {message}", path.display()))
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Default)]
pub struct LintConfig {
    pub rules: HashMap<String, RuleSetting>,
}

pub fn parse_config(text: &str, path: &Path) -> Result<LintConfig, ConfigError> {
    let data: Table = text.parse().map_err(|e| ConfigError::new(path, e))?;
    let mut config = LintConfig::default();

    let entries: Vec<(&String, &Value)> = match data.get("rules") {
        None => return Ok(config),
        Some(Value::Table(rules)) => rules.iter().collect(),
        // JavaScript treats arrays and dates as objects too: they are accepted, but name no rule.
        Some(Value::Array(items)) if items.is_empty() => Vec::new(),
        Some(Value::Array(_)) => return Err(ConfigError::new(path, "unknown rule id 0")),
        Some(Value::Datetime(_)) => Vec::new(),
        Some(_) => return Err(ConfigError::new(path, "[rules] must be a table")),
    };

    for (id, value) in entries {
        if !is_rule_id(id) {
            return Err(ConfigError::new(path, format!("unknown rule id {id}")));
        }
        // A table ([rules.XM402]) holds rule options; its presence alone doesn't change severity.
        if matches!(
            value,
            Value::Table(_) | Value::Array(_) | Value::Datetime(_)
        ) {
            continue;
        }
        let setting = value
            .as_str()
            .and_then(|s| SETTINGS.iter().find(|(name, _)| *name == s))
            .map(|(_, setting)| *setting);
        let Some(setting) = setting else {
            let names: Vec<&str> = SETTINGS.iter().map(|(name, _)| *name).collect();
            return Err(ConfigError::new(
                path,
                format!("{id} must be one of {}", names.join(", ")),
            ));
        };
        config.rules.insert(id.clone(), setting);
    }
    Ok(config)
}

/// Finds and parses the nearest xxmi.toml at or above a directory, caching per directory.
#[derive(Default)]
pub struct ConfigLoader {
    cache: RefCell<HashMap<PathBuf, Rc<LintConfig>>>,
}

impl ConfigLoader {
    pub fn for_directory(&self, dir: &Path) -> Result<Rc<LintConfig>, ConfigError> {
        if let Some(cached) = self.cache.borrow().get(dir) {
            return Ok(Rc::clone(cached));
        }

        let candidate = dir.join("xxmi.toml");
        let config = match fs::read(&candidate) {
            Ok(bytes) => parse_config(&String::from_utf8_lossy(&bytes), &candidate)?,
            Err(_) => match dir.parent() {
                Some(parent) => return self.cache_as(dir, self.for_directory(parent)?),
                None => LintConfig::default(),
            },
        };
        self.cache_as(dir, Rc::new(config))
    }

    fn cache_as(&self, dir: &Path, config: Rc<LintConfig>) -> Result<Rc<LintConfig>, ConfigError> {
        self.cache
            .borrow_mut()
            .insert(dir.to_path_buf(), Rc::clone(&config));
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<LintConfig, ConfigError> {
        parse_config(text, Path::new("xxmi.toml"))
    }

    #[test]
    fn reads_severities_and_off() {
        let config = parse("[rules]\nXM204 = \"off\"\nXM201 = \"warning\"\n").unwrap();
        assert_eq!(config.rules["XM204"], RuleSetting::Off);
        assert_eq!(config.rules["XM201"], RuleSetting::Level(Severity::Warning));
    }

    #[test]
    fn rule_option_tables_are_skipped() {
        assert!(parse("[rules.XM402]\nmax = 3\n").unwrap().rules.is_empty());
    }

    #[test]
    fn rejects_bad_ids_and_values() {
        let id = parse("[rules]\nXM2 = \"off\"\n").unwrap_err().to_string();
        assert_eq!(id, "xxmi.toml: unknown rule id XM2");
        let value = parse("[rules]\nXM201 = \"loud\"\n")
            .unwrap_err()
            .to_string();
        assert_eq!(
            value,
            "xxmi.toml: XM201 must be one of off, error, warning, info, hint"
        );
        let table = parse("rules = 1\n").unwrap_err().to_string();
        assert_eq!(table, "xxmi.toml: [rules] must be a table");
    }
}
