//! `xxmi capabilities [--json]`: what this build offers, for tools that bundle the binary.

use serde::Serialize;
use xxmi_core::profile::{Dialect, PROFILES, Profile};
use xxmi_core::rules::RULES;

use crate::args::{self, OptionSpec};
use crate::error::CliError;

const OPTIONS: [OptionSpec; 2] = [args::flag("json"), args::flag("help")];

const HELP: &str = concat!(
    "xxmi capabilities [--json]\n",
    "\n",
    "Lists the rules of this build (id, default severity, title, whether a quick fix exists),\n",
    "its profiles (id, name, dialect, whether it ships a library), the lint output schema versions\n",
    "and the version of this binary. --json prints it as one JSON line."
);

const DOC_BASE: &str = "https://github.com/xxmi-lang/xxmi-lang/blob/main/docs/02-diagnostics.md";

/// Bumped when a key of the `--json` document changes meaning or goes away.
const CAPABILITIES_VERSION: u32 = 1;

/// The `version` values `xxmi lint --format json` can emit.
const LINT_SCHEMA_VERSIONS: [u32; 1] = [1];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    capabilities_version: u32,
    version: &'static str,
    lint_schema_versions: [u32; 1],
    profiles: Vec<ProfileInfo>,
    rules: Vec<RuleInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileInfo {
    id: &'static str,
    name: &'static str,
    dialect: Dialect,
    has_library: bool,
}

impl From<&'static Profile> for ProfileInfo {
    fn from(profile: &'static Profile) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            dialect: profile.dialect,
            has_library: profile.snapshot.is_some(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleInfo {
    id: &'static str,
    default_severity: &'static str,
    title: &'static str,
    doc_url: String,
    has_fix: bool,
}

fn capabilities() -> Capabilities {
    Capabilities {
        capabilities_version: CAPABILITIES_VERSION,
        version: env!("CARGO_PKG_VERSION"),
        lint_schema_versions: LINT_SCHEMA_VERSIONS,
        profiles: PROFILES.iter().map(ProfileInfo::from).collect(),
        rules: RULES
            .iter()
            .map(|rule| RuleInfo {
                id: rule.id,
                default_severity: rule.severity.as_str(),
                title: rule.description,
                doc_url: format!("{DOC_BASE}#{}", rule.id.to_lowercase()),
                has_fix: rule.has_fix,
            })
            .collect(),
    }
}

fn format_text(capabilities: &Capabilities) -> String {
    capabilities
        .rules
        .iter()
        .map(|r| format!("{}  {:<7}  {}", r.id, r.default_severity, r.title))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn run(raw_args: &[String]) -> Result<i32, CliError> {
    let parsed = args::parse(raw_args, &OPTIONS)?;
    if parsed.flag("help") {
        println!("{HELP}");
        return Ok(0);
    }
    if !parsed.positionals.is_empty() {
        return Err(CliError::Usage("capabilities takes no paths".to_string()));
    }

    let capabilities = capabilities();
    if parsed.flag("json") {
        let json = serde_json::to_string(&capabilities)
            .map_err(|error| CliError::Internal(error.to_string()))?;
        println!("{json}");
    } else {
        println!("{}", format_text(&capabilities));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_has_the_documented_shape() {
        let value = serde_json::to_value(capabilities()).unwrap();
        assert_eq!(value["capabilitiesVersion"], 1);
        assert_eq!(value["lintSchemaVersions"], serde_json::json!([1]));
        assert_eq!(
            value["profiles"][0],
            serde_json::json!({ "id": "xxmi", "name": "XXMI (generic)", "dialect": "xxmi", "hasLibrary": false })
        );
        assert_eq!(value["profiles"][1]["dialect"], "gimi");
        assert_eq!(value["profiles"][4]["id"], "zzmi");
        assert_eq!(value["profiles"][4]["hasLibrary"], true);

        let first = &value["rules"][0];
        assert_eq!(first["id"], "XM001");
        assert_eq!(first["defaultSeverity"], "error");
        assert_eq!(first["hasFix"], false);
        assert_eq!(
            first["docUrl"],
            "https://github.com/xxmi-lang/xxmi-lang/blob/main/docs/02-diagnostics.md#xm001"
        );
    }

    #[test]
    fn exactly_the_fixing_rules_have_fix() {
        let with_fix: Vec<&str> = RULES.iter().filter(|r| r.has_fix).map(|r| r.id).collect();
        assert_eq!(
            with_fix,
            [
                "XM003", "XM005", "XM006", "XM101", "XM102", "XM103", "XM105", "XM106", "XM110",
                "XM114", "XM115", "XM201", "XM202", "XM203", "XM305", "XM403"
            ]
        );
    }
}
