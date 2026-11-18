use std::path::Path;
use std::process::exit;

/// What the TypeScript build bundles, relative to this crate: the merged spec (generated JSON
/// plus overlays) and the checked-in library snapshot.
const EMBEDDED: [(&str, &str); 2] = [
    ("XXMI_SPEC_JSON", "../../../packages/core/dist/spec.json"),
    (
        "XXMI_ZZMI_SNAPSHOT_JSON",
        "../../../packages/core/snapshots/zzmi.json",
    ),
];

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");

    for (variable, relative) in EMBEDDED {
        let path = Path::new(&manifest_dir).join(relative);
        println!("cargo:rerun-if-changed={}", path.display());

        let Ok(path) = path.canonicalize() else {
            eprintln!(
                "error: {} is missing\n\
                 run `pnpm install && pnpm build` at the repository root first (it writes \
                 packages/core/dist/spec.json from spec/generated and spec/overlay)",
                path.display()
            );
            exit(1);
        };
        println!("cargo:rustc-env={variable}={}", path.display());
    }
}
