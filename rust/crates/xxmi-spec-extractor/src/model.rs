//! The spec's JSON shape.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Documentation fields only overlays provide, in the order they were first set.
#[derive(Debug, Clone, Default)]
pub struct Docs(pub Vec<(String, String)>);

impl Docs {
    pub fn set(&mut self, field: &str, value: String) {
        match self.0.iter_mut().find(|(name, _)| name == field) {
            Some(slot) => slot.1 = value,
            None => self.0.push((field.to_string(), value)),
        }
    }
}

impl Serialize for Docs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (field, value) in &self.0 {
            map.serialize_entry(field, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Docs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DocsVisitor;

        impl<'de> Visitor<'de> for DocsVisitor {
            type Value = Docs;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of documentation strings")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Docs, A::Error> {
                let mut docs = Docs::default();
                while let Some((field, value)) = map.next_entry::<String, String>()? {
                    docs.set(&field, value);
                }
                Ok(docs)
            }
        }

        deserializer.deserialize_map(DocsVisitor)
    }
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchKind {
    Exact,
    Prefix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecKey {
    pub name: String,
    pub value_type: String,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeatable: Option<bool>,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecDynamicKeys {
    pub kind: String,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecRemovedKey {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecSection {
    pub name: String,
    pub prefix: bool,
    pub kind: String,
    pub allows_bare_lines: bool,
    pub allows_duplicate_keys: bool,
    pub keys: Vec<SpecKey>,
    pub removed_keys: Vec<SpecRemovedKey>,
    pub dynamic_keys: Vec<SpecDynamicKeys>,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecBuiltinSection {
    pub name: String,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecCommandValue {
    pub name: String,
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecCommand {
    pub name: String,
    pub kind: String,
    #[serde(rename = "match")]
    pub match_kind: MatchKind,
    pub values: Vec<SpecCommandValue>,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indirect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecResourceMember {
    pub name: String,
    pub target: String,
    pub args: Vec<String>,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecFunction {
    pub name: String,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecOperatorToken {
    pub token: String,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecPrecedenceLevel {
    pub level: usize,
    pub group: String,
    pub associativity: String,
    pub unary: bool,
    pub operators: Vec<String>,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecOperators {
    pub tokens: Vec<SpecOperatorToken>,
    pub precedence: Vec<SpecPrecedenceLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecEnumValue {
    pub name: String,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecEnum {
    pub name: String,
    pub values: Vec<SpecEnumValue>,
    pub source: String,
    pub xxmi: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub gimi: bool,
    #[serde(flatten)]
    pub docs: Docs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub repository: String,
    pub commit: String,
    #[serde(rename = "commitDate")]
    pub commit_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecMeta {
    pub extractor_version: u32,
    pub dll: GitInfo,
    /// Vanilla 3DMigoto checkout used to decide `xxmi` flags.
    pub baseline: GitInfo,
    /// XXMI-Libs-Package at v0.1.0 (vanilla GIMI DLL code), used to decide `gimi` flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gimi: Option<GitInfo>,
}

/// A spec without its `meta`: what one DLL checkout yields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecBody {
    pub sections: Vec<SpecSection>,
    pub builtin_sections: Vec<SpecBuiltinSection>,
    pub commands: Vec<SpecCommand>,
    pub resource_members: Vec<SpecResourceMember>,
    pub functions: Vec<SpecFunction>,
    pub operators: SpecOperators,
    pub enums: Vec<SpecEnum>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub meta: SpecMeta,
    #[serde(flatten)]
    pub body: SpecBody,
}
