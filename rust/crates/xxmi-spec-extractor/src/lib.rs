//! Extracts the 3DMigoto / XXMI language spec from DLL source checkouts.

mod commands;
mod cpp;
mod dll;
mod flags;
pub mod load;
mod model;
mod operators;
mod output;
pub mod overlay;
mod sections;
mod text;

use std::path::Path;

pub use cpp::{AnchorNotFound, CppFile};
pub use dll::{DllSource, ExtractError, Extraction, git_info};
pub use model::*;
pub use output::serialize_spec;

/// Bump when the output shape or extraction rules change, so regenerated specs are expected.
pub const EXTRACTOR_VERSION: u32 = 3;

/// Extracts one DLL. `strict` fails on any missing anchor (use for the XXMI DLL).
pub fn extract_body(
    dll: DllSource,
    strict: bool,
) -> Result<(SpecBody, Vec<String>), AnchorNotFound> {
    let x = Extraction::new(dll, strict);

    // A partial checkout (e.g. a CI sparse checkout) would otherwise silently drop keys.
    for component in x.dll.missing_components.clone() {
        x.step((), || {
            Err(AnchorNotFound::new(
                &format!("{component}/"),
                "directory",
                "check out the whole repo or add it to the sparse checkout",
            ))
        })?;
    }

    let (operators, patterns) = operators::extract_operators(&x)?;
    let commands::Commands {
        commands,
        resource_members,
    } = commands::extract_commands(&x)?;
    let body = SpecBody {
        sections: sections::extract_sections(&x)?,
        builtin_sections: sections::extract_builtin_sections(&x)?,
        commands,
        resource_members,
        functions: x.step(Vec::new(), || {
            commands::functions(x.dll.file("CommandList.cpp")?, &patterns)
        })?,
        operators,
        enums: operators::extract_enums(&x)?,
    };
    Ok((body, x.warnings()))
}

/// Records, per section, the keys vanilla reads that the XXMI DLL no longer does.
pub fn record_removed_keys(body: &mut SpecBody, baseline: &SpecBody) {
    for section in &mut body.sections {
        let Some(vanilla) = baseline
            .sections
            .iter()
            .find(|s| s.name.to_lowercase() == section.name.to_lowercase())
        else {
            continue;
        };
        let known: Vec<String> = section.keys.iter().map(|k| k.name.to_lowercase()).collect();
        section.removed_keys = vanilla
            .keys
            .iter()
            .filter(|k| !known.contains(&k.name.to_lowercase()))
            .map(|k| SpecRemovedKey {
                name: k.name.clone(),
                source: k.source.clone(),
            })
            .collect();
    }
}

pub struct ExtractOptions<'a> {
    pub dll_root: &'a Path,
    pub baseline_root: &'a Path,
    /// Overrides for checkouts without git metadata (tests, materialized sources).
    pub dll_git: Option<GitInfo>,
    pub baseline_git: Option<GitInfo>,
}

pub struct ExtractResult {
    pub spec: Spec,
    /// Anchors missing from the baseline (expected: XXMI-only tables).
    pub baseline_warnings: Vec<String>,
}

pub fn extract_spec(options: ExtractOptions) -> Result<ExtractResult, ExtractError> {
    let (mut body, _) = extract_body(DllSource::load(options.dll_root)?, true)?;
    let (mut baseline, baseline_warnings) =
        extract_body(DllSource::load(options.baseline_root)?, false)?;

    flags::flag_xxmi(&mut body, &mut baseline);
    record_removed_keys(&mut body, &baseline);
    let meta = SpecMeta {
        extractor_version: EXTRACTOR_VERSION,
        dll: match options.dll_git {
            Some(git) => git,
            None => git_info(options.dll_root)?,
        },
        baseline: match options.baseline_git {
            Some(git) => git,
            None => git_info(options.baseline_root)?,
        },
    };
    Ok(ExtractResult {
        spec: Spec { meta, body },
        baseline_warnings,
    })
}
