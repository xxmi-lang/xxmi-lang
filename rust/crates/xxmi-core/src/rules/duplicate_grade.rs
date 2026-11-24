//! Grades an XM107 duplicate whose first copy is in another file by what ignoring it costs.

use crate::diagnostic::Severity;
use crate::model::mesh::is_mesh_slot;
use crate::model::{IniFile, SectionModel, SymbolKind};

use super::RuleContext;

pub enum Grade {
    Identical,
    MeshResource,
    MeshList,
    Other,
}

impl Grade {
    pub fn severity(&self) -> Severity {
        match self {
            Self::Identical => Severity::Info,
            Self::MeshResource | Self::MeshList => Severity::Error,
            Self::Other => Severity::Warning,
        }
    }

    pub fn note(&self) -> Option<&'static str> {
        match self {
            Self::Identical => Some("identical to the first"),
            Self::MeshResource => Some("ignored mesh resource"),
            Self::MeshList => Some("ignored mesh command list"),
            Self::Other => None,
        }
    }
}

pub fn grade(
    context: &RuleContext,
    duplicate: (&IniFile, &SectionModel),
    winner: (&IniFile, &SectionModel),
) -> Grade {
    if normalized_lines(duplicate) == normalized_lines(winner) {
        Grade::Identical
    } else if is_mesh_resource(context, duplicate) {
        Grade::MeshResource
    } else if is_mesh_list(duplicate) {
        Grade::MeshList
    } else {
        Grade::Other
    }
}

/// Key case and whitespace don't matter to the DLL; comment lines never reach the model.
fn normalized_lines((file, section): (&IniFile, &SectionModel)) -> Vec<String> {
    let collapse = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");

    section
        .lines
        .iter()
        .map(|line| match (line.scanned.key, line.scanned.value) {
            (Some(key), Some(value)) => format!(
                "{}={}",
                collapse(file.slice(key)).to_lowercase(),
                collapse(file.slice(value))
            ),
            _ => collapse(file.slice(line.scanned.content)),
        })
        .collect()
}

/// A Resource a `vb0`-`vb2` or `ib` line binds anywhere in its package,
/// or one backed by a `.buf` or `.ib` file.
fn is_mesh_resource(context: &RuleContext, (file, section): (&IniFile, &SectionModel)) -> bool {
    if section.kind != Some(SymbolKind::Resource) {
        return false;
    }

    let bound = context
        .workspace
        .usage_for(file)
        .mesh_bound
        .contains(&section.qualified_name.to_lowercase());
    bound || has_mesh_filename(file, section)
}

fn has_mesh_filename(file: &IniFile, section: &SectionModel) -> bool {
    section
        .lines
        .iter()
        .filter(|line| line.command.as_deref() == Some("filename"))
        .filter_map(|line| line.scanned.value)
        .map(|value| file.slice(value).to_lowercase())
        .any(|value| value.ends_with(".buf") || value.ends_with(".ib"))
}

/// A CommandList or TextureOverride that binds any `vb*` or `ib`,
/// or a TextureOverride that skips or draws.
fn is_mesh_list((file, section): (&IniFile, &SectionModel)) -> bool {
    let kind = section.kind;
    if !matches!(
        kind,
        Some(SymbolKind::CommandList | SymbolKind::TextureOverride)
    ) {
        return false;
    }

    let binds_mesh = section
        .lines
        .iter()
        .any(|line| line.command.as_deref().is_some_and(is_mesh_slot));
    let draws = kind == Some(SymbolKind::TextureOverride)
        && section
            .lines
            .iter()
            .filter(|line| line.command.as_deref() == Some("handling"))
            .filter_map(|line| line.scanned.value)
            .any(|value| {
                matches!(
                    file.slice(value).to_lowercase().as_str(),
                    "skip" | "draw" | "drawindexed"
                )
            });
    binds_mesh || draws
}
