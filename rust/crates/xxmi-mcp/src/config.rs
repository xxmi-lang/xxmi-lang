//! Server settings from environment variables (docs/06-mcp-server.md "As built").

use std::thread;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub host: String,
    pub data_dir: String,
    pub admin_token: Option<String>,
    pub github_token: Option<String>,
    pub poll_interval_min: u64,
    pub indexed_releases: u64,
    pub indexed_prereleases: u64,
    pub git_cache_max_mb: u64,
    pub rate_limit_per_min: u64,
    pub rate_limit_burst: u64,
    pub trusted_proxy_hops: usize,
    pub max_body_bytes: usize,
    pub max_lint_bytes: usize,
    pub max_lint_files: usize,
    pub tool_timeout_ms: u64,
    pub search_timeout_ms: u64,
    pub indexer_enabled: bool,
    /// Package snapshots kept in memory, least recently used out first.
    pub package_cache_entries: usize,
    /// Tokio worker threads: `WORKER_THREADS`, else the available parallelism capped at 4.
    pub worker_threads: usize,
}

fn int(
    env: &dyn Fn(&str) -> Option<String>,
    name: &str,
    fallback: u64,
    min: u64,
) -> Result<u64, String> {
    let Some(raw) = env(name).filter(|raw| !raw.trim().is_empty()) else {
        return Ok(fallback);
    };
    // JavaScript's Number() accepts surrounding space and a leading plus; keep those inputs working.
    match raw.trim().trim_start_matches('+').parse::<u64>() {
        Ok(value) if value >= min => Ok(value),
        _ => Err(format!("{name} must be an integer >= {min}, got \"{raw}\"")),
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(&|name| std::env::var(name).ok())
    }

    pub fn from_lookup(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let releases_fallback = int(env, "INDEXED_TAGS", 3, 0)?;
        let port = int(env, "PORT", 3000, 1)?;
        let default_workers = thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(4);

        Ok(Self {
            port: u16::try_from(port)
                .map_err(|_| format!("PORT must be an integer >= 1, got \"{port}\""))?,
            host: env("HOST").unwrap_or_else(|| "0.0.0.0".to_string()),
            data_dir: env("DATA_DIR").unwrap_or_else(|| "./data".to_string()),
            admin_token: env("ADMIN_TOKEN").filter(|v| !v.is_empty()),
            github_token: env("GITHUB_TOKEN").filter(|v| !v.is_empty()),
            poll_interval_min: int(env, "POLL_INTERVAL_MIN", 15, 0)?,
            indexed_releases: int(env, "INDEXED_RELEASES", releases_fallback, 0)?,
            indexed_prereleases: int(env, "INDEXED_PRERELEASES", 2, 0)?,
            git_cache_max_mb: int(env, "GIT_CACHE_MAX_MB", 512, 1)?,
            rate_limit_per_min: int(env, "RATE_LIMIT_PER_MIN", 60, 0)?,
            rate_limit_burst: int(env, "RATE_LIMIT_BURST", 20, 1)?,
            trusted_proxy_hops: int(env, "TRUSTED_PROXY_HOPS", 0, 0)? as usize,
            max_body_bytes: int(env, "MAX_BODY_BYTES", 3_000_000, 1024)? as usize,
            max_lint_bytes: int(env, "MAX_LINT_BYTES", 1_000_000, 1)? as usize,
            max_lint_files: int(env, "MAX_LINT_FILES", 200, 1)? as usize,
            tool_timeout_ms: int(env, "TOOL_TIMEOUT_MS", 15_000, 100)?,
            search_timeout_ms: int(env, "SEARCH_TIMEOUT_MS", 2_000, 100)?,
            indexer_enabled: env("INDEXER").as_deref() != Some("off"),
            package_cache_entries: int(env, "PACKAGE_CACHE_ENTRIES", 64, 1)? as usize,
            worker_threads: int(env, "WORKER_THREADS", default_workers as u64, 1)? as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from(pairs: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_lookup(&|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn defaults_match_the_typescript_server() {
        let config = from(&[]).unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.max_body_bytes, 3_000_000);
        assert_eq!(config.rate_limit_burst, 20);
        assert!(config.indexer_enabled);
    }

    #[test]
    fn indexed_tags_is_the_old_name_of_indexed_releases() {
        assert_eq!(from(&[("INDEXED_TAGS", "5")]).unwrap().indexed_releases, 5);
        let both = from(&[("INDEXED_TAGS", "5"), ("INDEXED_RELEASES", "2")]).unwrap();
        assert_eq!(both.indexed_releases, 2);
    }

    #[test]
    fn bad_values_name_the_variable() {
        assert_eq!(
            from(&[("MAX_BODY_BYTES", "10")]).unwrap_err(),
            "MAX_BODY_BYTES must be an integer >= 1024, got \"10\""
        );
        assert!(from(&[("PORT", "abc")]).is_err());
    }
}
