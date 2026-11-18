//! `xxmi index`: builds a library symbol snapshot from a package.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use xxmi_core::defaults::bundled_lookup;
use xxmi_core::paths::{PathResolver, display_path, resolve_path};
use xxmi_core::workspace::Workspace;
use xxmi_core::workspace::package::find_package_root;
use xxmi_core::workspace::snapshot::{SnapshotSource, SymbolSnapshot};

use crate::args::{self, OptionSpec};
use crate::error::CliError;

const OPTIONS: [OptionSpec; 3] = [
    args::string("out"),
    args::string("name"),
    args::flag("help"),
];

const HELP: &str = concat!(
    "xxmi index <package dir> --out FILE [--name NAME]\n",
    "\n",
    "Builds a library symbol snapshot from a package (the folder holding d3dx.ini): the sections,\n",
    "globals and namespaces of d3dx.ini and every file it includes."
);

/// Trimmed stdout of `git -C root ...`, or empty when git or the repository is missing.
fn git(root: &Path, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(String::new, |output| {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        })
}

/// Returns the process exit code.
pub fn run(raw_args: &[String]) -> Result<i32, CliError> {
    let parsed = args::parse(raw_args, &OPTIONS)?;
    if parsed.flag("help") {
        println!("{HELP}");
        return Ok(0);
    }

    let (Some(dir), Some(out)) = (
        parsed.positionals.first(),
        parsed.string("out").filter(|o| !o.is_empty()),
    ) else {
        return Err(CliError::Usage(
            "xxmi index needs a package dir and --out FILE".to_string(),
        ));
    };
    let Some(root) = find_package_root(&resolve_path(Path::new(dir)), &PathResolver::default())
    else {
        return Err(CliError::Usage(format!(
            "no d3dx.ini found at or above {dir}"
        )));
    };

    let mut workspace = Workspace::new(bundled_lookup(), &[]);
    workspace.load_package(&root)?;
    let repository = git(&root, &["remote", "get-url", "origin"]);
    let source = SnapshotSource {
        repository: repository
            .strip_suffix(".git")
            .unwrap_or(&repository)
            .to_string(),
        commit: git(&root, &["rev-parse", "HEAD"]),
    };
    let name = parsed.string("name").unwrap_or("package").to_string();
    let snapshot = SymbolSnapshot::build(&workspace, &root, name, source);

    let out = resolve_path(Path::new(out));
    write_snapshot(&out, &snapshot)?;
    eprintln!(
        "Indexed {} files: {} sections, {} globals, {} namespaces → {}",
        workspace.files().len(),
        snapshot.sections.len(),
        snapshot.variables.len(),
        snapshot.namespaces.len(),
        display_path(&out, &env::current_dir()?)
    );
    Ok(0)
}

fn write_snapshot(out: &Path, snapshot: &SymbolSnapshot) -> Result<(), CliError> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let json =
        serde_json::to_string_pretty(snapshot).map_err(|e| CliError::Internal(e.to_string()))?;
    fs::write(out, format!("{json}\n"))?;
    Ok(())
}
