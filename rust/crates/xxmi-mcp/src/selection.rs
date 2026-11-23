//! Which refs of a repo get indexed, per role: the head always, and this picks the tags.
//! See docs/06-mcp-server.md "Versions".

use std::cmp::Ordering;

use crate::catalog::VersionRef;
use crate::repos::RepoRole;
use crate::upstream::RemoteTag;
use crate::upstream::releases::{ReleasesState, is_stale};

pub struct SelectionLimits {
    pub releases: usize,
    pub prereleases: usize,
}

/// Whether tags (and so the GitHub releases lookup) matter for `role`; the baseline is head-only.
pub fn tracks_tags(role: RepoRole) -> bool {
    matches!(role, RepoRole::Dll | RepoRole::Package | RepoRole::Tools)
}

/// Digit runs and the runs between them.
fn runs(text: &str) -> Vec<&str> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut previous: Option<bool> = None;
    for (i, c) in text.char_indices() {
        let digit = c.is_ascii_digit();
        if previous.is_some_and(|p| p != digit) {
            runs.push(&text[start..i]);
            start = i;
        }
        previous = Some(digit);
    }
    if start < text.len() {
        runs.push(&text[start..]);
    }
    runs
}

/// `localeCompare` for the short ASCII runs of a tag name:
/// letters ignore case first, and a lowercase letter sorts before its capital on a tie.
fn compare_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| b.cmp(a))
}

/// Numeric runs compare as numbers, so `v1.10` is newer than `v1.9`.
pub fn compare_tag_names(a: &str, b: &str) -> Ordering {
    let (left, right) = (runs(a), runs(b));
    for (x, y) in left.iter().zip(&right) {
        let numeric = |run: &str| run.starts_with(|c: char| c.is_ascii_digit());
        let order = if numeric(x) && numeric(y) {
            compare_numbers(x, y)
        } else {
            compare_text(x, y)
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

fn compare_numbers(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Newest first: by publish date when both have one, else by tag name.
pub fn newest_first(a: &VersionRef, b: &VersionRef) -> Ordering {
    if let (Some(x), Some(y)) = (&a.published_at, &b.published_at)
        && x != y
    {
        return y.cmp(x);
    }
    compare_tag_names(&b.name, &a.name)
}

fn classify(tag: &RemoteTag, state: Option<&ReleasesState>) -> Option<VersionRef> {
    let meta = state.and_then(|s| s.releases.get(&tag.name));
    if meta.is_some_and(|m| m.draft) {
        return None;
    }
    let kind = match meta {
        None => "tag",
        Some(m) if m.prerelease => "prerelease",
        Some(_) => "release",
    };
    Some(VersionRef {
        name: tag.name.clone(),
        sha: tag.sha.clone(),
        kind: kind.to_string(),
        tag: Some(tag.name.clone()),
        published_at: meta.and_then(|m| m.published_at.clone()),
        committed_at: None,
        subject: None,
    })
}

fn of_kind(versions: &[VersionRef], kind: &str) -> Vec<VersionRef> {
    let mut found: Vec<VersionRef> = versions
        .iter()
        .filter(|v| v.kind == kind)
        .cloned()
        .collect();
    found.sort_by(newest_first);
    found
}

/// The tag versions to index, newest first:
/// the newest releases, then the pre-releases newer than the newest release.
/// Tags GitHub knows nothing about only count when the repo has no releases at all,
/// or while the release map is stale (a new tag may be a pre-release).
pub fn select_versions(
    role: RepoRole,
    tags: &[RemoteTag],
    state: Option<&ReleasesState>,
    limits: &SelectionLimits,
) -> Vec<VersionRef> {
    if !tracks_tags(role) {
        return Vec::new();
    }
    let all: Vec<VersionRef> = tags.iter().filter_map(|t| classify(t, state)).collect();
    let releases = of_kind(&all, "release");
    let prereleases = of_kind(&all, "prerelease");
    if releases.is_empty() && prereleases.is_empty() {
        let mut plain = of_kind(&all, "tag");
        plain.truncate(limits.releases);
        return plain;
    }

    let names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    let unclassified = if is_stale(state, &names) {
        of_kind(&all, "tag")
    } else {
        Vec::new()
    };
    let newest = releases.first();
    let mut newer: Vec<VersionRef> = prereleases
        .into_iter()
        .chain(unclassified)
        .filter(|v| newest.is_none_or(|n| newest_first(v, n) == Ordering::Less))
        .collect();
    newer.sort_by(newest_first);

    let mut chosen: Vec<VersionRef> = releases
        .into_iter()
        .take(limits.releases)
        .chain(newer.into_iter().take(limits.prereleases))
        .collect();
    chosen.sort_by(newest_first);
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::releases::ReleaseMeta;

    fn tag(name: &str) -> RemoteTag {
        RemoteTag {
            name: name.to_string(),
            sha: format!("{name:0<40}").replace(|c: char| !c.is_ascii_hexdigit(), "0"),
        }
    }

    fn state(tags: &[&str], prereleases: &[&str]) -> ReleasesState {
        let releases = tags
            .iter()
            .map(|name| {
                let meta = ReleaseMeta {
                    name: None,
                    prerelease: prereleases.contains(name),
                    draft: false,
                    published_at: None,
                };
                (name.to_string(), meta)
            })
            .collect();
        ReleasesState {
            tags: tags.iter().map(|t| t.to_string()).collect(),
            synced: true,
            releases,
            failed_at: None,
        }
    }

    fn names(versions: &[VersionRef]) -> Vec<&str> {
        versions.iter().map(|v| v.name.as_str()).collect()
    }

    const LIMITS: SelectionLimits = SelectionLimits {
        releases: 2,
        prereleases: 1,
    };

    #[test]
    fn numeric_runs_compare_as_numbers() {
        assert_eq!(compare_tag_names("v1.10", "v1.9"), Ordering::Greater);
        assert_eq!(compare_tag_names("v1.2", "v1.2.1"), Ordering::Less);
    }

    #[test]
    fn picks_the_newest_releases_and_newer_prereleases() {
        let tags = [
            tag("v1.0"),
            tag("v1.1"),
            tag("v1.2"),
            tag("v1.3-beta"),
            tag("v0.9-beta"),
        ];
        let all = ["v1.0", "v1.1", "v1.2", "v1.3-beta", "v0.9-beta"];
        let releases = state(&all, &["v1.3-beta", "v0.9-beta"]);

        let chosen = select_versions(RepoRole::Dll, &tags, Some(&releases), &LIMITS);

        assert_eq!(names(&chosen), vec!["v1.3-beta", "v1.2", "v1.1"]);
    }

    #[test]
    fn plain_tags_count_only_without_releases() {
        let tags = [tag("v1"), tag("v2"), tag("v3")];
        let none = select_versions(RepoRole::Package, &tags, None, &LIMITS);
        assert_eq!(names(&none), vec!["v3", "v2"]);
        assert!(select_versions(RepoRole::Baseline, &tags, None, &LIMITS).is_empty());
    }
}
