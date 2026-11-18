use std::path::Path;
use std::process::exit;

/// The merged spec the TypeScript build bundles (generated JSON plus overlays).
const BUNDLED_SPEC: &str = "../../../packages/core/dist/spec.json";

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let spec = Path::new(&manifest_dir).join(BUNDLED_SPEC);
    println!("cargo:rerun-if-changed={}", spec.display());

    let Ok(spec) = spec.canonicalize() else {
        eprintln!(
            "error: the bundled spec is missing: {}\n\
             run `pnpm install && pnpm build` at the repository root first (it writes \
             packages/core/dist/spec.json from spec/generated and spec/overlay)",
            spec.display()
        );
        exit(1);
    };
    println!("cargo:rustc-env=XXMI_SPEC_JSON={}", spec.display());
}
