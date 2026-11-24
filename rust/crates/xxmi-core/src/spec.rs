//! The parts of the merged spec the lint rules read.
//!
use serde::Deserialize;

fn available() -> bool {
    true
}

/// What every spec entry carries: where it came from, plus the documentation overlays add.
#[derive(Debug, Deserialize)]
pub struct Meta {
    /// `<file>:<symbol>` in the XXMI DLL source, or `manual`.
    #[serde(default)]
    pub source: String,
    /// In the XXMI fork but not in vanilla 3DMigoto.
    #[serde(default)]
    pub xxmi: bool,
    /// Present in the GIMI DLL (XXMI-Libs-Package v0.1.0); overlay-only entries count as present.
    #[serde(default = "available")]
    pub gimi: bool,
    pub doc: Option<String>,
    pub example: Option<String>,
    pub since: Option<String>,
    pub deprecated: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub sections: Vec<SpecSection>,
    pub builtin_sections: Vec<SpecBuiltinSection>,
    pub commands: Vec<SpecCommand>,
    #[serde(default)]
    pub resource_members: Vec<SpecResourceMember>,
    #[serde(default)]
    pub functions: Vec<SpecFunction>,
    pub operators: SpecOperators,
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
    #[serde(flatten)]
    pub meta: Meta,
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
    #[serde(flatten)]
    pub meta: Meta,
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
    /// The first argument is a resource holding the indirect arguments.
    #[serde(default)]
    pub indirect: bool,
    #[serde(flatten)]
    pub meta: Meta,
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

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Draw => "draw",
            Self::Flow => "flow",
            Self::Declaration => "declaration",
            Self::Prefix => "prefix",
        }
    }
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
    #[serde(flatten)]
    pub meta: Meta,
}

/// `resource`: `ResourceCopyTarget` (`Res->Region`); `shader`: `ShaderTarget` (`ps->...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberTarget {
    Resource,
    Shader,
}

impl MemberTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resource => "resource",
            Self::Shader => "shader",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SpecResourceMember {
    /// Without the `->`.
    pub name: String,
    pub target: MemberTarget,
    pub args: Vec<String>,
    #[serde(flatten)]
    pub meta: Meta,
}

#[derive(Debug, Deserialize)]
pub struct SpecFunction {
    pub name: String,
    #[serde(flatten)]
    pub meta: Meta,
}

#[derive(Debug, Deserialize)]
pub struct SpecOperators {
    pub tokens: Vec<SpecOperatorToken>,
    pub precedence: Vec<SpecPrecedenceLevel>,
}

#[derive(Debug, Deserialize)]
pub struct SpecOperatorToken {
    pub token: String,
    #[serde(flatten)]
    pub meta: Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Associativity {
    Left,
    Right,
}

impl Associativity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SpecPrecedenceLevel {
    /// 0 binds tightest.
    pub level: u32,
    /// The DLL's name for the group of operators at this level.
    pub group: String,
    pub associativity: Associativity,
    pub unary: bool,
    pub operators: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SpecEnumValue {
    pub name: String,
}

impl Spec {
    /// Parses a merged spec (the JSON that `xxmi_spec_extractor::load::merged_spec_json` builds).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Gives the spec the process lifetime, which the section and command references of a model
    /// need; a lint run loads at most one.
    pub fn leak(self) -> &'static Spec {
        Box::leak(Box::new(self))
    }
}
