//! Builds the final spec of one DLL commit:
//! the extractor's output plus the overlay shipped with this server build,
//! the same merge the bundled spec gets.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Semaphore;
use xxmi_spec_extractor::overlay::{OverlayFile, apply_overlays, parse_overlay};
use xxmi_spec_extractor::{EXTRACTOR_VERSION, ExtractOptions, GitInfo, extract_spec};

pub const BUILD_TIMEOUT: Duration = Duration::from_secs(120);

/// Builds running at once.
/// Each holds a whole spec model, so the memory bound matters more than the speed.
const MAX_PARALLEL_BUILDS: usize = 1;

const OVERLAYS: [(&str, &str); 3] = [
    (
        "commands.toml",
        include_str!("../../../../spec/overlay/commands.toml"),
    ),
    (
        "enums.toml",
        include_str!("../../../../spec/overlay/enums.toml"),
    ),
    (
        "sections.toml",
        include_str!("../../../../spec/overlay/sections.toml"),
    ),
];

/// One extractor input: a materialized checkout and the commit it came from.
#[derive(Clone, Debug)]
pub struct SourceInput {
    pub dir: PathBuf,
    pub commit: String,
    pub commit_date: String,
    pub repository: String,
}

#[derive(Clone, Debug)]
pub struct BuildInput {
    pub dll: SourceInput,
    pub baseline: SourceInput,
}

pub struct BuiltSpec {
    pub spec: Value,
    pub warnings: Vec<String>,
}

fn overlays() -> Vec<OverlayFile> {
    OVERLAYS
        .iter()
        .filter_map(|(file, text)| parse_overlay(file, text).ok())
        .collect()
}

/// FNV-1a: stable across builds, which `DefaultHasher` does not promise.
fn fnv_hex(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")[..8].to_string()
}

/// Names everything a stored spec depends on besides the sources,
/// so a deploy that changes the release, the extractor or the overlay rebuilds the specs.
/// The `rust-` prefix keeps the two servers from trusting each other's specs.
pub fn builder_id() -> String {
    let overlay_text: String = OVERLAYS.iter().map(|(_, text)| *text).collect();
    format!(
        "rust-{}+extractor{EXTRACTOR_VERSION}+overlay{}",
        crate::VERSION,
        fnv_hex(&overlay_text)
    )
}

/// Materialized sources are not git checkouts, so the extractor gets the commit info explicitly.
fn git_info(source: &SourceInput) -> GitInfo {
    GitInfo {
        repository: source.repository.clone(),
        commit: source.commit.clone(),
        commit_date: source.commit_date.clone(),
    }
}

/// Entries an older DLL lacks are skipped with a problem instead of failing the build.
pub fn build_version_spec(input: &BuildInput) -> Result<BuiltSpec, String> {
    let extracted = extract_spec(ExtractOptions {
        dll_root: &input.dll.dir,
        baseline_root: &input.baseline.dir,
        dll_git: Some(git_info(&input.dll)),
        baseline_git: Some(git_info(&input.baseline)),
        gimi_root: None,
        gimi_git: None,
    })
    .map_err(|e| e.to_string())?;

    let mut spec = extracted.spec;
    let problems = apply_overlays(&mut spec, &overlays());
    let warnings = extracted
        .baseline_warnings
        .iter()
        .map(|w| format!("baseline: {w}"))
        .chain(problems.iter().map(|p| format!("overlay: {p}")))
        .collect();

    let spec = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
    Ok(BuiltSpec { spec, warnings })
}

type BuildFn = dyn Fn(BuildInput) -> Result<BuiltSpec, String> + Send + Sync;

/// Runs builds off the request workers, a bounded number at a time.
#[derive(Clone)]
pub struct SpecBuilder {
    build: Arc<BuildFn>,
    slots: Arc<Semaphore>,
    timeout: Duration,
}

impl SpecBuilder {
    pub fn new() -> Self {
        Self::with(|input| build_version_spec(&input), BUILD_TIMEOUT)
    }

    /// A builder around `build`; tests replace the extractor with it.
    pub fn with(
        build: impl Fn(BuildInput) -> Result<BuiltSpec, String> + Send + Sync + 'static,
        timeout: Duration,
    ) -> Self {
        Self {
            build: Arc::new(build),
            slots: Arc::new(Semaphore::new(MAX_PARALLEL_BUILDS)),
            timeout,
        }
    }

    /// The permit stays with the thread, not the caller:
    /// a build that outlives its timeout still occupies its slot until it really ends.
    pub async fn build(&self, input: BuildInput) -> Result<BuiltSpec, String> {
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let build = self.build.clone();
        let task = tokio::task::spawn_blocking(move || {
            let built = build(input);
            drop(permit);
            built
        });

        match tokio::time::timeout(self.timeout, task).await {
            Ok(joined) => joined.map_err(|e| e.to_string())?,
            Err(_) => Err(format!(
                "spec build timed out after {} ms",
                self.timeout.as_millis()
            )),
        }
    }
}

impl Default for SpecBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_overlay_file_is_embedded() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../spec/overlay");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".toml"))
            .collect();
        on_disk.sort();

        let embedded: Vec<&str> = OVERLAYS.iter().map(|(file, _)| *file).collect();
        assert_eq!(on_disk, embedded);
        assert_eq!(overlays().len(), OVERLAYS.len());
    }

    #[test]
    fn the_builder_id_names_the_overlay() {
        assert!(builder_id().starts_with("rust-"));
        assert_eq!(fnv_hex("a"), fnv_hex("a"));
        assert_ne!(fnv_hex("a"), fnv_hex("b"));
    }

    #[tokio::test]
    async fn a_build_that_never_ends_times_out() {
        let builder = SpecBuilder::with(
            |_| {
                std::thread::sleep(Duration::from_millis(300));
                Err("late".into())
            },
            Duration::from_millis(20),
        );
        let source = SourceInput {
            dir: PathBuf::new(),
            commit: String::new(),
            commit_date: String::new(),
            repository: String::new(),
        };
        let input = BuildInput {
            dll: source.clone(),
            baseline: source,
        };

        let error = builder.build(input).await.err().unwrap();

        assert!(error.contains("timed out"), "{error}");
    }
}
