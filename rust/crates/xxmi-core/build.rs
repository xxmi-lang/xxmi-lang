use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

use xxmi_spec_extractor::load::merged_spec_json;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let repo = Path::new(&manifest_dir).join("../../..");

    embed_spec(&repo.join("spec"));
    embed_snapshot(&repo.join("snapshots/zzmi.json"));
}

/// Merges spec/generated and spec/overlay into OUT_DIR, so the embedded spec cannot go stale.
fn embed_spec(spec_root: &Path) {
    println!("cargo:rerun-if-changed={}", spec_root.display());

    let json = merged_spec_json(spec_root).unwrap_or_else(|problems| {
        let list: Vec<String> = problems.iter().map(|p| format!("  - {p}")).collect();
        fail(&format!("invalid spec:\n{}", list.join("\n")));
    });
    let out =
        PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("spec.json");
    fs::write(&out, json).unwrap_or_else(|e| fail(&format!("{}: {e}", out.display())));
    println!("cargo:rustc-env=XXMI_SPEC_JSON={}", out.display());
}

/// The checked-in library snapshot, rebuilt with `xxmi index`.
fn embed_snapshot(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());

    let Ok(path) = path.canonicalize() else {
        fail(&format!("{} is missing", path.display()));
    };
    println!("cargo:rustc-env=XXMI_ZZMI_SNAPSHOT_JSON={}", path.display());
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    exit(1);
}
