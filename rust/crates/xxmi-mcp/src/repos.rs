//! The repositories the server tracks. The allowlist is code, not input: the server never fetches
//! a repository that isn't listed here (docs/06-mcp-server.md "Security and limits").

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoRole {
    Dll,
    Package,
    Tools,
    /// bo3b/3Dmigoto, indexed at its head only and not offered by package or library tools.
    Baseline,
}

impl RepoRole {
    pub fn as_str(self) -> &'static str {
        match self {
            RepoRole::Dll => "dll",
            RepoRole::Package => "package",
            RepoRole::Tools => "tools",
            RepoRole::Baseline => "baseline",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TrackedRepo {
    pub alias: &'static str,
    pub owner: &'static str,
    pub name: &'static str,
    pub role: RepoRole,
    /// Package repos: the game profile this package belongs to.
    pub game: Option<&'static str>,
    /// Package repos: the folder inside the repository that holds `d3dx.ini`.
    pub package_dir: Option<&'static str>,
}

impl TrackedRepo {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Folder name under the data volume, safe on every file system.
    pub fn dir_name(&self) -> String {
        format!("{}__{}", self.owner, self.name)
    }
}

/// The git remote of a tracked repo.
pub fn repo_url(repo: &TrackedRepo) -> String {
    format!("https://github.com/{}.git", repo.full_name())
}

pub static TRACKED_REPOS: [TrackedRepo; 4] = [
    TrackedRepo {
        alias: "dll",
        owner: "SpectrumQT",
        name: "XXMI-Libs-Package",
        role: RepoRole::Dll,
        game: None,
        package_dir: None,
    },
    TrackedRepo {
        alias: "zzmi",
        owner: "leotorrez",
        name: "ZZMI-Package",
        role: RepoRole::Package,
        game: Some("zzmi"),
        package_dir: Some("ZZMI"),
    },
    TrackedRepo {
        alias: "xxmitools",
        owner: "leotorrez",
        name: "XXMITools",
        role: RepoRole::Tools,
        game: None,
        package_dir: None,
    },
    TrackedRepo {
        alias: "migoto",
        owner: "bo3b",
        name: "3Dmigoto",
        role: RepoRole::Baseline,
        game: None,
        package_dir: None,
    },
];

/// Finds a tracked repo by `owner/name` or alias, case-insensitively.
pub fn find_repo(query: &str) -> Option<&'static TrackedRepo> {
    let lower = query.trim().to_lowercase();
    TRACKED_REPOS
        .iter()
        .find(|r| r.alias == lower || r.full_name().to_lowercase() == lower)
}

pub fn game_repo(game: &str) -> Option<&'static TrackedRepo> {
    let lower = game.trim().to_lowercase();
    TRACKED_REPOS
        .iter()
        .find(|r| r.role == RepoRole::Package && r.game == Some(lower.as_str()))
}

pub fn dll_repo() -> &'static TrackedRepo {
    &TRACKED_REPOS[0]
}

pub fn baseline_repo() -> &'static TrackedRepo {
    &TRACKED_REPOS[3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_alias_and_full_name() {
        assert_eq!(find_repo(" ZZMI ").unwrap().alias, "zzmi");
        assert_eq!(
            find_repo("spectrumqt/xxmi-libs-package").unwrap().alias,
            "dll"
        );
        assert!(find_repo("nope/nope").is_none());
    }

    #[test]
    fn games_map_to_package_repos() {
        assert_eq!(game_repo("ZZMI").unwrap().package_dir, Some("ZZMI"));
        assert!(game_repo("gimi").is_none());
    }
}
