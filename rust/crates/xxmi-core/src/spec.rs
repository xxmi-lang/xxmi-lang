//! The parts of the bundled spec the lint rules read, loaded once from the embedded JSON.

use std::sync::OnceLock;

use serde::Deserialize;

const BUNDLED_SPEC: &str = include_str!(env!("XXMI_SPEC_JSON"));

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub sections: Vec<SpecSection>,
    pub commands: Vec<SpecCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionKind {
    CommandList,
    Regular,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecSection {
    pub name: String,
    /// Matches any section whose name starts with `name`, case-insensitively.
    pub prefix: bool,
    pub kind: SectionKind,
    pub allows_bare_lines: bool,
    pub keys: Vec<SpecKey>,
    #[serde(default)]
    pub removed_keys: Vec<SpecKey>,
    #[serde(default)]
    pub dynamic_keys: Vec<SpecDynamicKeys>,
}

/// A key, or a removed key: only the name is read.
#[derive(Debug, Deserialize)]
pub struct SpecKey {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SpecDynamicKeys {
    pub kind: DynamicKeyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DynamicKeyKind {
    Variable,
    IniParam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandKind {
    General,
    Draw,
    Flow,
    Declaration,
    Prefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchKind {
    Exact,
    Prefix,
}

#[derive(Debug, Deserialize)]
pub struct SpecCommand {
    pub name: String,
    pub kind: CommandKind,
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
    /// `value`: the DLL matches the right-hand side, so the name is not a key.
    #[serde(default)]
    pub form: Option<CommandForm>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandForm {
    Value,
}

impl SpecCommand {
    pub fn is_key_command(&self) -> bool {
        self.form != Some(CommandForm::Value)
    }
}

pub fn spec() -> &'static Spec {
    static SPEC: OnceLock<Spec> = OnceLock::new();
    SPEC.get_or_init(|| serde_json::from_str(BUNDLED_SPEC).expect("the bundled spec parses"))
}
