//! The parts of the merged spec the lint rules read (shape of packages/core/src/spec/types.ts).
//!
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub sections: Vec<SpecSection>,
    pub builtin_sections: Vec<SpecBuiltinSection>,
    pub commands: Vec<SpecCommand>,
    pub enums: Vec<SpecEnum>,
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
    #[serde(default)]
    pub allows_duplicate_keys: bool,
    pub keys: Vec<SpecKey>,
    #[serde(default)]
    pub removed_keys: Vec<SpecRemovedKey>,
    #[serde(default)]
    pub dynamic_keys: Vec<SpecDynamicKeys>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecKey {
    pub name: String,
    /// `enum:<EnumTableName>` for enum-valued keys, else a plain type name.
    pub value_type: String,
    /// The DLL reads every occurrence of the key.
    #[serde(default)]
    pub repeatable: bool,
}

#[derive(Debug, Deserialize)]
pub struct SpecRemovedKey {
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

#[derive(Debug, Deserialize)]
pub struct SpecBuiltinSection {
    pub name: String,
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
#[serde(rename_all = "camelCase")]
pub struct SpecCommand {
    pub name: String,
    pub kind: CommandKind,
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
    /// Keyword values the DLL recognises specially (`handling = skip`, ...).
    #[serde(default)]
    pub values: Vec<SpecCommandValue>,
    pub arg_count: Option<usize>,
    /// `value`: the DLL matches the right-hand side, so the name is not a key.
    #[serde(default)]
    pub form: Option<CommandForm>,
}

#[derive(Debug, Deserialize)]
pub struct SpecCommandValue {
    pub name: String,
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
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

#[derive(Debug, Deserialize)]
pub struct SpecEnum {
    pub name: String,
    pub values: Vec<SpecEnumValue>,
}

#[derive(Debug, Deserialize)]
pub struct SpecEnumValue {
    pub name: String,
}

impl Spec {
    /// Parses a merged spec (the JSON shape of `dist/spec.json`).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Gives the spec the process lifetime, which the section and command references of a model
    /// need; a lint run loads at most one.
    pub fn leak(self) -> &'static Spec {
        Box::leak(Box::new(self))
    }
}
