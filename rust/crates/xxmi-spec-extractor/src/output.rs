//! The spec/generated/*.json files: JSON.stringify(value, null, 2) plus a newline.

use serde::Serialize;

use crate::model::{
    Spec, SpecBuiltinSection, SpecCommand, SpecEnum, SpecFunction, SpecResourceMember, SpecSection,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SectionsFile<'a> {
    sections: &'a [SpecSection],
    builtin_sections: &'a [SpecBuiltinSection],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandsFile<'a> {
    commands: &'a [SpecCommand],
    resource_members: &'a [SpecResourceMember],
    functions: &'a [SpecFunction],
}

#[derive(Serialize)]
struct EnumsFile<'a> {
    enums: &'a [SpecEnum],
}

fn json<T: Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("spec serializes");
    text.push('\n');
    text
}

/// File name ↦ contents for spec/generated/. Pure: same spec, same bytes.
pub fn serialize_spec(spec: &Spec) -> Vec<(&'static str, String)> {
    let body = &spec.body;
    vec![
        ("meta.json", json(&spec.meta)),
        (
            "sections.json",
            json(&SectionsFile {
                sections: &body.sections,
                builtin_sections: &body.builtin_sections,
            }),
        ),
        (
            "commands.json",
            json(&CommandsFile {
                commands: &body.commands,
                resource_members: &body.resource_members,
                functions: &body.functions,
            }),
        ),
        ("operators.json", json(&body.operators)),
        ("enums.json", json(&EnumsFile { enums: &body.enums })),
    ]
}
