//! Regenerates spec/generated/*.json from DLL source checkouts.
//!
//!   XXMI_DLL_SRC=<XXMI-Libs-Package> MIGOTO_DLL_SRC=<3Dmigoto> [GIMI_DLL_SRC=<XXMI-Libs-Package @ v0.1.0>]
//!   xxmi-spec-extract [--check]
//!
//! --dll DIR / --baseline DIR / --gimi DIR / --out DIR override the environment and the default output
//! (spec/generated under the working directory).
//! --check writes nothing and exits 1 if the output would differ from what is on disk.
//! --full FILE also writes the merged spec
//! (the overlays of DIR/overlay, DIR from --spec-root, default spec) as compact JSON. --dll-commit,
//! --dll-commit-date, --dll-repository
//! (and the --baseline-* and --gimi-* forms) stand in for git metadata when a source is not a git checkout.
//!
//! Exit codes: 0 ok, 1 --check found a difference, 2 usage error, 3 extraction failed.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xxmi_spec_extractor::overlay::{apply_overlays, read_overlays};
use xxmi_spec_extractor::{ExtractError, ExtractOptions, GitInfo, extract_spec, serialize_spec};

const USAGE: &str = "usage: XXMI_DLL_SRC=<XXMI-Libs-Package> MIGOTO_DLL_SRC=<3Dmigoto> [GIMI_DLL_SRC=<v0.1.0 checkout>] xxmi-spec-extract [--check]";

const VALUE_FLAGS: [&str; 15] = [
    "--dll",
    "--baseline",
    "--gimi",
    "--out",
    "--full",
    "--dll-commit",
    "--dll-commit-date",
    "--dll-repository",
    "--baseline-commit",
    "--baseline-commit-date",
    "--baseline-repository",
    "--gimi-commit",
    "--gimi-commit-date",
    "--gimi-repository",
    "--spec-root",
];

struct Args {
    values: HashMap<String, String>,
    check: bool,
}

fn parse_args() -> Option<Args> {
    let mut values = HashMap::new();
    let mut check = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--check" {
            check = true;
        } else if VALUE_FLAGS.contains(&arg.as_str()) {
            values.insert(arg, args.next()?);
        } else {
            return None;
        }
    }
    Some(Args { values, check })
}

impl Args {
    fn path(&self, flag: &str, variable: &str) -> Option<PathBuf> {
        self.values
            .get(flag)
            .cloned()
            .or_else(|| env::var(variable).ok())
            .map(PathBuf::from)
    }

    /// Git metadata given on the command line, for a source that is not a git checkout.
    fn git(&self, prefix: &str) -> Option<GitInfo> {
        let get = |suffix: &str| self.values.get(&format!("--{prefix}-{suffix}"));
        let commit = get("commit")?;
        Some(GitInfo {
            repository: get("repository").cloned().unwrap_or_default(),
            commit: commit.clone(),
            commit_date: get("commit-date").cloned().unwrap_or_default(),
        })
    }
}

fn main() -> ExitCode {
    let Some(args) = parse_args() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let (Some(dll), Some(baseline)) = (
        args.path("--dll", "XXMI_DLL_SRC"),
        args.path("--baseline", "MIGOTO_DLL_SRC"),
    ) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match run(
        &args,
        &dll,
        &baseline,
        args.path("--gimi", "GIMI_DLL_SRC").as_deref(),
    ) {
        Ok(code) => code,
        Err(ExtractError::Anchor(error)) => {
            eprintln!("Extraction failed: {error}");
            eprintln!(
                "The DLL source moved. Update the anchor in rust/crates/xxmi-spec-extractor/src/."
            );
            ExitCode::from(3)
        }
        Err(error) => {
            eprintln!("Extraction failed: {error}");
            ExitCode::from(3)
        }
    }
}

fn run(
    args: &Args,
    dll: &Path,
    baseline: &Path,
    gimi: Option<&Path>,
) -> Result<ExitCode, ExtractError> {
    let mut result = extract_spec(ExtractOptions {
        dll_root: dll,
        baseline_root: baseline,
        dll_git: args.git("dll"),
        baseline_git: args.git("baseline"),
        gimi_root: gimi,
        gimi_git: args.git("gimi"),
    })?;
    for warning in &result.baseline_warnings {
        eprintln!("baseline (not XXMI): {warning}");
    }
    for warning in &result.gimi_warnings {
        eprintln!("gimi baseline (not at v0.1.0): {warning}");
    }
    for removed in &result.gimi_only {
        eprintln!("gimi baseline only (removed since): {removed}");
    }
    let dll_info = &result.spec.meta.dll;
    let origin = if dll_info.repository.is_empty() {
        dll.display().to_string()
    } else {
        dll_info.repository.clone()
    };
    eprintln!("Extracted from {origin} @ {}", dll_info.commit);

    let files = serialize_spec(&result.spec);
    let out = PathBuf::from(
        args.values
            .get("--out")
            .map_or("spec/generated", String::as_str),
    );
    let code = if args.check {
        check(&files, &out)
    } else {
        write_files(&files, &out)?;
        ExitCode::SUCCESS
    };

    if let Some(full) = args.values.get("--full") {
        write_full(&mut result.spec, args, Path::new(full))?;
    }
    Ok(code)
}

fn check(files: &[(&str, String)], out: &Path) -> ExitCode {
    let mut stale = false;
    for (name, text) in files {
        let path = out.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            eprintln!("differs: {}", path.display());
            stale = true;
        }
    }
    ExitCode::from(u8::from(stale))
}

fn write_files(files: &[(&str, String)], out: &Path) -> Result<(), ExtractError> {
    let io = |path: &Path, e: std::io::Error| ExtractError::Io(format!("{}: {e}", path.display()));
    fs::create_dir_all(out).map_err(|e| io(out, e))?;
    for (name, text) in files {
        fs::write(out.join(name), text).map_err(|e| io(&out.join(name), e))?;
    }
    eprintln!("Wrote {} files to {}", files.len(), out.display());
    Ok(())
}

/// The merged spec (generated spec plus overlays), as `merged_spec_json` produces it for the build.
fn write_full(
    spec: &mut xxmi_spec_extractor::Spec,
    args: &Args,
    path: &Path,
) -> Result<(), ExtractError> {
    let root = args
        .values
        .get("--spec-root")
        .map_or("spec", String::as_str);
    let (overlays, mut problems) = read_overlays(Path::new(root));
    problems.extend(apply_overlays(spec, &overlays));
    for problem in &problems {
        eprintln!("overlay: {problem}");
    }
    let text = serde_json::to_string(spec).expect("spec serializes");
    fs::write(path, text).map_err(|e| ExtractError::Io(format!("{}: {e}", path.display())))
}
