//! Reads spec/generated/*.json back into the model and merges spec/overlay/*.toml over it:
//! the bundled spec the other crates embed, built without the TypeScript toolchain.

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::model::{
    Spec, SpecBody, SpecBuiltinSection, SpecCommand, SpecEnum, SpecFunction, SpecMeta,
    SpecOperators, SpecResourceMember, SpecSection,
};
use crate::overlay::{apply_overlays, read_overlays};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SectionsFile {
    sections: Vec<SpecSection>,
    builtin_sections: Vec<SpecBuiltinSection>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandsFile {
    commands: Vec<SpecCommand>,
    resource_members: Vec<SpecResourceMember>,
    functions: Vec<SpecFunction>,
}

#[derive(Deserialize)]
struct EnumsFile {
    enums: Vec<SpecEnum>,
}

fn read_json<T: DeserializeOwned>(dir: &Path, name: &str) -> Result<T, String> {
    let path = dir.join(name);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The generated spec under `root/generated`, without overlays.
pub fn load_generated_spec(root: &Path) -> Result<Spec, String> {
    let dir = root.join("generated");
    let sections: SectionsFile = read_json(&dir, "sections.json")?;
    let commands: CommandsFile = read_json(&dir, "commands.json")?;
    let operators: SpecOperators = read_json(&dir, "operators.json")?;
    let enums: EnumsFile = read_json(&dir, "enums.json")?;

    Ok(Spec {
        meta: read_json::<SpecMeta>(&dir, "meta.json")?,
        body: SpecBody {
            sections: sections.sections,
            builtin_sections: sections.builtin_sections,
            commands: commands.commands,
            resource_members: commands.resource_members,
            functions: commands.functions,
            operators,
            enums: enums.enums,
        },
    })
}

/// The merged spec of `root` (generated JSON plus overlays) as compact JSON,
/// or every problem found while reading and merging.
pub fn merged_spec_json(root: &Path) -> Result<String, Vec<String>> {
    let mut spec = load_generated_spec(root).map_err(|e| vec![e])?;
    let (overlays, mut problems) = read_overlays(root);
    problems.extend(apply_overlays(&mut spec, &overlays));
    if !problems.is_empty() {
        return Err(problems);
    }

    serde_json::to_string(&spec).map_err(|e| vec![e.to_string()])
}
