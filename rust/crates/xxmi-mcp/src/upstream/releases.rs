//! GitHub release metadata: the one thing `git` can't tell us (release vs pre-release). The only
//! GitHub API call the server makes, and only when a repo's tag set changed, so the
//! unauthenticated limit (60/hour per IP) is never in play.

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::logger::Logger;
use crate::repos::TrackedRepo;

const API: &str = "https://api.github.com";
const TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_AFTER_FAILURE_MS: u64 = 60 * 60_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseMeta {
    pub name: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub published_at: Option<String>,
}

pub struct ReleaseInfo {
    pub tag: String,
    pub meta: ReleaseMeta,
}

/// What is persisted per repo in `state.json`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasesState {
    /// Tag names the `releases` map was last fetched for; empty until the first success.
    pub tags: Vec<String>,
    pub synced: bool,
    pub releases: BTreeMap<String, ReleaseMeta>,
    /// Epoch ms of the last failed lookup; cleared by a success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<u64>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    name: Option<String>,
    prerelease: bool,
    draft: bool,
    published_at: Option<String>,
}

/// The newest 30 releases of `repo`; `token` is sent only when configured.
pub fn fetch_releases(repo: &TrackedRepo, token: Option<&str>) -> Result<Vec<ReleaseInfo>, String> {
    let url = format!("{API}/repos/{}/releases?per_page=30", repo.full_name());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent
        .get(&url)
        .header("user-agent", "xxmi-mcp")
        .header("accept", "application/vnd.github+json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }

    let mut response = request.call().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    if status >= 400 {
        let limited = response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v == "0");
        let note = if limited {
            " (API rate limit reached)"
        } else {
            ""
        };
        return Err(format!("GitHub {status} for {url}{note}"));
    }

    let list: Vec<ApiRelease> = response
        .body_mut()
        .read_json()
        .map_err(|_| format!("unexpected releases response (HTTP {status})"))?;
    Ok(list
        .into_iter()
        .map(|r| ReleaseInfo {
            tag: r.tag_name,
            meta: ReleaseMeta {
                name: r.name,
                prerelease: r.prerelease,
                draft: r.draft,
                published_at: r.published_at,
            },
        })
        .collect())
}

fn same_set(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && b.iter().all(|t| a.contains(t))
}

/// True while the stored release map doesn't cover the current tags (tags may be unclassified).
pub fn is_stale(state: Option<&ReleasesState>, tags: &[String]) -> bool {
    state.is_none_or(|s| !s.synced || !same_set(&s.tags, tags))
}

fn should_look_up(state: Option<&ReleasesState>, tags: &[String], now: u64, force: bool) -> bool {
    if !is_stale(state, tags) {
        return force;
    }
    match state.and_then(|s| s.failed_at) {
        Some(failed_at) if !force => now.saturating_sub(failed_at) >= RETRY_AFTER_FAILURE_MS,
        _ => true,
    }
}

/// Where release metadata comes from; GitHub in production, a fixed list in tests.
pub trait ReleaseLookup: Send + Sync {
    fn releases(
        &self,
        repo: &'static TrackedRepo,
    ) -> impl Future<Output = Result<Vec<ReleaseInfo>, String>> + Send;
}

pub struct GithubReleases {
    pub token: Option<String>,
}

impl ReleaseLookup for GithubReleases {
    async fn releases(&self, repo: &'static TrackedRepo) -> Result<Vec<ReleaseInfo>, String> {
        let token = self.token.clone();
        tokio::task::spawn_blocking(move || fetch_releases(repo, token.as_deref()))
            .await
            .map_err(|e| e.to_string())?
    }
}

pub struct RefreshOptions<'a> {
    pub logger: &'a Logger,
    pub now: u64,
    pub force: bool,
}

/// The release state to use for `tags`: the stored one while the tag set is unchanged, else a
/// fresh lookup. A failed lookup keeps the previous map and is retried at most hourly.
pub async fn refresh_releases(
    lookup: &impl ReleaseLookup,
    repo: &'static TrackedRepo,
    tags: &[String],
    previous: Option<&ReleasesState>,
    options: &RefreshOptions<'_>,
) -> ReleasesState {
    if let Some(previous) = previous
        && !should_look_up(Some(previous), tags, options.now, options.force)
    {
        return previous.clone();
    }

    match lookup.releases(repo).await {
        Ok(infos) => ReleasesState {
            tags: tags.to_vec(),
            synced: true,
            releases: infos.into_iter().map(|i| (i.tag, i.meta)).collect(),
            failed_at: None,
        },
        Err(error) => {
            // Log the first failure of a streak only: the hourly retries would repeat it.
            if previous.is_none_or(|p| p.failed_at.is_none()) {
                options.logger.warn(
                    "release lookup failed; new tags stay unclassified",
                    json!({ "repo": repo.full_name(), "error": error }),
                );
            }
            ReleasesState {
                tags: previous.map(|p| p.tags.clone()).unwrap_or_default(),
                synced: previous.is_some_and(|p| p.synced),
                releases: previous.map(|p| p.releases.clone()).unwrap_or_default(),
                failed_at: Some(options.now),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(tags: &[&str], synced: bool) -> ReleasesState {
        ReleasesState {
            tags: tags.iter().map(|t| t.to_string()).collect(),
            synced,
            ..ReleasesState::default()
        }
    }

    fn names(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn a_state_is_stale_until_synced_for_the_same_tags() {
        assert!(is_stale(None, &names(&["v1"])));
        assert!(is_stale(Some(&state(&["v1"], false)), &names(&["v1"])));
        assert!(is_stale(Some(&state(&["v1"], true)), &names(&["v1", "v2"])));
        assert!(!is_stale(
            Some(&state(&["v2", "v1"], true)),
            &names(&["v1", "v2"])
        ));
    }

    #[test]
    fn failed_lookups_are_retried_hourly_unless_forced() {
        let mut failed = state(&[], false);
        failed.failed_at = Some(1_000);
        let tags = names(&["v1"]);

        assert!(!should_look_up(Some(&failed), &tags, 1_000 + 60_000, false));
        assert!(should_look_up(
            Some(&failed),
            &tags,
            1_000 + RETRY_AFTER_FAILURE_MS,
            false
        ));
        assert!(should_look_up(Some(&failed), &tags, 2_000, true));
    }
}
