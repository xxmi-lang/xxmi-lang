//! Picks the package index a `game` / `ref` pair refers to, falling back to the bundled snapshot.

use xxmi_core::workspace::snapshot::SymbolSnapshot;

use crate::bundled::bundled_snapshot;
use crate::catalog::ResolvedRef;
use crate::packages::{LoadedIndex, load_package};
use crate::repos::{TRACKED_REPOS, game_repo};
use crate::specs::short;

use super::shared::{ToolContext, ToolError};

pub struct PackageChoice {
    pub snapshot: &'static SymbolSnapshot,
    /// Absent while only the bundled snapshot is available.
    pub resolved: Option<ResolvedRef>,
    pub loaded: Option<LoadedIndex>,
    /// Shown to the caller when the answer is degraded.
    pub note: Option<String>,
}

pub fn supported_games() -> Vec<&'static str> {
    TRACKED_REPOS.iter().filter_map(|r| r.game).collect()
}

/// `need_docs` parses the package for doc comments and usage; the linter only needs the symbols.
pub fn choose_package(
    ctx: &ToolContext,
    game: &str,
    wanted: Option<&str>,
    need_docs: bool,
) -> Result<PackageChoice, ToolError> {
    let Some(repo) = game_repo(game) else {
        return Err(ToolError::new(format!(
            "Unknown game \"{game}\". Supported: {}.",
            supported_games().join(", ")
        )));
    };
    if ctx.catalog.is_ready(repo) {
        return indexed_choice(ctx, repo, wanted, need_docs);
    }
    bundled_choice(repo, wanted, need_docs)
}

fn indexed_choice(
    ctx: &ToolContext,
    repo: &'static crate::repos::TrackedRepo,
    wanted: Option<&str>,
    need_docs: bool,
) -> Result<PackageChoice, ToolError> {
    let resolved = ctx.catalog.resolve(repo, wanted)?;
    let loaded = if need_docs {
        Some(load_package(&resolved).map_err(io_error)?)
    } else {
        None
    };
    let snapshot = ctx
        .packages
        .snapshot(&resolved, loaded.as_ref())
        .map_err(io_error)?;
    Ok(PackageChoice {
        snapshot,
        resolved: Some(resolved),
        loaded,
        note: None,
    })
}

fn bundled_choice(
    repo: &'static crate::repos::TrackedRepo,
    wanted: Option<&str>,
    need_docs: bool,
) -> Result<PackageChoice, ToolError> {
    if let Some(wanted) = wanted.filter(|w| !w.is_empty() && *w != "latest") {
        return Err(ToolError::new(format!(
            "{} is still warming up, so \"{wanted}\" is not available yet.",
            repo.name
        )));
    }
    let Some(snapshot) = bundled_snapshot(repo.game.unwrap_or_default()) else {
        return Err(ToolError::new(format!(
            "{} is still warming up. Try again in a minute.",
            repo.name
        )));
    };

    let docs = if need_docs {
        " Doc comments and usage are not available from it."
    } else {
        ""
    };
    let note = format!(
        "Index is warming up: answering from the snapshot bundled with the server ({}).{docs}",
        short(&snapshot.source.commit)
    );
    Ok(PackageChoice {
        snapshot,
        resolved: None,
        loaded: None,
        note: Some(note),
    })
}

fn io_error(error: std::io::Error) -> ToolError {
    ToolError::internal(format!("package load failed: {error}"))
}
