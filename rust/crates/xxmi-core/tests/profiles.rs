//! Game profiles: which MI a package is checked against, and that packages stay apart.

use std::fs;
use std::path::{Path, PathBuf};

use xxmi_core::defaults::{bundled_lookup, bundled_snapshots};
use xxmi_core::lint::{ConfigLoader, LintError, LintOptions, lint_paths, lint_texts};
use xxmi_core::paths::{PathResolver, resolve_path};
use xxmi_core::profile::{self, Profile};
use xxmi_core::workspace::Workspace;

const CALL: &str = "[Present]\nrun = CommandList\\Foo\\Shared\n";
const MOD: &str = CALL;

fn virtual_files(files: &[(&str, &str)]) -> Vec<(PathBuf, String)> {
    files
        .iter()
        .map(|(path, text)| (PathBuf::from(path), text.to_string()))
        .collect()
}

/// The profile of each file, as the linter sees them (in-memory files, d3dx.ini first).
fn profiles_of(files: &[(&str, &str)], forced: Option<&'static Profile>) -> Vec<(String, String)> {
    let files = virtual_files(files);
    let resolver = PathResolver::in_memory(files.iter().map(|(path, _)| path.as_path()));
    let mut workspace = Workspace::with_resolver(bundled_lookup(), bundled_snapshots(), resolver);
    workspace.set_profile_override(forced);

    let mut ordered: Vec<&(PathBuf, String)> = files.iter().collect();
    ordered.sort_by_key(|(path, _)| path.file_name().is_some_and(|n| n != "d3dx.ini"));
    for (path, text) in &ordered {
        workspace.set_overlay(path, text.clone());
    }
    for (path, text) in &ordered {
        let full = resolve_path(path);
        let fallback = full.parent().unwrap().to_path_buf();
        workspace
            .open_document(&full, text.clone(), &fallback)
            .unwrap();
    }

    workspace
        .files()
        .iter()
        .map(|file| {
            let name = file
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (name, workspace.profile_for(file).id.to_string())
        })
        .collect()
}

fn profile_ids(files: &[(&str, &str)]) -> Vec<String> {
    profiles_of(files, None)
        .into_iter()
        .map(|(_, id)| id)
        .collect()
}

/// The files that carry a diagnostic with this id.
fn flagged(files: &[(&str, &str)], options: &LintOptions, code: &str) -> Vec<String> {
    let result = lint_texts(&virtual_files(files), options, &ConfigLoader::disabled()).unwrap();
    result
        .files
        .iter()
        .filter(|f| f.diagnostics.iter().any(|d| d.id == code))
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect()
}

/// What `flagged` reports for a virtual path: files are resolved (a drive is added on Windows).
fn resolved(path: &str) -> String {
    resolve_path(Path::new(path)).to_string_lossy().into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xxmi-profiles-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_package_folder_named_after_the_mi_picks_it() {
    let ids = profile_ids(&[
        ("/v/ZZMI/d3dx.ini", "[Constants]\n"),
        ("/v/ZZMI/Mods/a.ini", MOD),
    ]);
    assert_eq!(ids, ["zzmi", "zzmi"]);

    let ids = profile_ids(&[("/v/wwmi/d3dx.ini", "[Constants]\n")]);
    assert_eq!(ids, ["wwmi"]);
}

/// One profile per library: the generic profile has none and is never picked by name.
fn library_ids() -> Vec<&'static str> {
    profile::PROFILES
        .iter()
        .filter(|p| p.snapshot.is_some())
        .map(|p| p.id)
        .collect()
}

fn mixed_case(id: &str) -> String {
    let (first, rest) = id.split_at(1);
    format!("{}{}", first.to_uppercase(), rest)
}

#[test]
fn every_library_profile_is_found_by_folder_name_in_any_case_and_by_core_folder() {
    for id in library_ids() {
        for folder in [id.to_uppercase(), mixed_case(id)] {
            let path = format!("/v/{folder}/d3dx.ini");
            assert_eq!(profile_ids(&[(&path, "")]), [id], "folder {folder}");
        }

        let core = format!("/v/install/Core/{}/Main.ini", id.to_uppercase());
        let files = [("/v/install/d3dx.ini", ""), (core.as_str(), "")];
        assert_eq!(profile_ids(&files), [id, id], "Core/{id}");
    }
}

#[test]
fn the_generic_id_in_a_folder_name_falls_through_to_references() {
    let zzmi = "[Present]\nrun = CommandList\\ZZMI\\SetTextures\n";
    let files = [("/v/XXMI/d3dx.ini", ""), ("/v/XXMI/Mods/a.ini", zzmi)];
    assert_eq!(profile_ids(&files), ["zzmi", "zzmi"]);

    let files = [("/v/XXMI/d3dx.ini", ""), ("/v/XXMI/Mods/a.ini", MOD)];
    assert_eq!(profile_ids(&files), ["xxmi", "xxmi"]);

    let files = [
        ("/v/install/d3dx.ini", ""),
        ("/v/install/Core/XXMI/Main.ini", ""),
        ("/v/install/Mods/a.ini", zzmi),
    ];
    assert_eq!(profile_ids(&files), ["zzmi", "zzmi", "zzmi"]);
}

#[test]
fn a_core_folder_for_the_mi_picks_it() {
    let ids = profile_ids(&[
        ("/v/install/d3dx.ini", "[Constants]\n"),
        ("/v/install/Core/SRMI/Main.ini", "[Constants]\n"),
        ("/v/install/Mods/a.ini", MOD),
    ]);
    assert_eq!(ids, ["srmi", "srmi", "srmi"]);
}

#[test]
fn references_to_a_namespace_only_one_library_declares_pick_it() {
    let wwmi = "[Present]\nrun = CommandList\\WWMIv1\\Initialize\n";
    assert_eq!(profile_ids(&[("/v/loose/a.ini", wwmi)]), ["wwmi"]);

    let zzmi = "[Present]\nrun = CommandList\\ZZMI\\SetTextures\n";
    assert_eq!(
        profile_ids(&[("/v/install/d3dx.ini", ""), ("/v/install/a.ini", zzmi)]),
        ["zzmi", "zzmi"]
    );
}

#[test]
fn ambiguous_or_missing_references_fall_back_to_the_generic_profile() {
    let both =
        "[Present]\nrun = CommandList\\WWMIv1\\Initialize\nrun = CommandList\\ZZMI\\SetTextures\n";
    assert_eq!(profile_ids(&[("/v/loose/a.ini", both)]), ["xxmi"]);
    assert_eq!(profile_ids(&[("/v/loose/a.ini", MOD)]), ["xxmi"]);

    // `Mouse` is declared by two libraries.
    let shared = "[Present]\nrun = CommandList\\Mouse\\Click\n";
    assert_eq!(profile_ids(&[("/v/loose/a.ini", shared)]), ["xxmi"]);
}

#[test]
fn the_folder_wins_over_references_and_the_setting_wins_over_both() {
    let wwmi = "[Present]\nrun = CommandList\\WWMIv1\\Initialize\n";
    let files = [("/v/ZZMI/d3dx.ini", ""), ("/v/ZZMI/Mods/a.ini", wwmi)];
    assert_eq!(profile_ids(&files), ["zzmi", "zzmi"]);

    let forced = profile::by_id("gimi");
    let ids: Vec<String> = profiles_of(&files, forced)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    assert_eq!(ids, ["gimi", "gimi"]);
}

#[test]
fn xxmi_toml_wins_over_the_folder_and_a_bad_id_is_a_config_error() {
    let dir = scratch("toml");
    let package = dir.join("ZZMI");
    fs::create_dir_all(package.join("Mods")).unwrap();
    fs::write(package.join("d3dx.ini"), "[Constants]\n").unwrap();
    fs::write(package.join("Mods/a.ini"), MOD).unwrap();
    fs::write(dir.join("xxmi.toml"), "profile = \"himi\"\n").unwrap();

    let mut workspace = Workspace::new(bundled_lookup(), bundled_snapshots());
    workspace.set_configs(ConfigLoader::default());
    let id = workspace
        .add_target(&package.join("Mods/a.ini"), &dir)
        .unwrap();
    let file = &workspace.files()[id];
    assert_eq!(workspace.profile_for(file).id, "himi");

    fs::write(dir.join("xxmi.toml"), "profile = \"nope\"\n").unwrap();
    let error = lint_paths(std::slice::from_ref(&dir), &LintOptions::bundled())
        .err()
        .unwrap();
    assert!(matches!(error, LintError::Config(_)));
    assert!(
        error.to_string().contains("unknown profile nope"),
        "{error}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_package_never_resolves_against_another_packages_files() {
    let defining = "namespace = Foo\n[CommandListShared]\n";
    let files = [
        ("/v/a/d3dx.ini", "[Constants]\n"),
        ("/v/a/Mods/def.ini", defining),
        ("/v/a/Mods/use.ini", CALL),
        ("/v/b/d3dx.ini", "[Constants]\n"),
        ("/v/b/Mods/use.ini", CALL),
    ];
    let options = LintOptions {
        profile: Some(profile::generic()),
        ..LintOptions::bundled()
    };

    // `Foo` is a namespace package b never declares: XM206, not XM201 (which needs a known namespace).
    assert_eq!(
        flagged(&files, &options, "XM206"),
        [resolved("/v/b/Mods/use.ini")]
    );
    assert!(flagged(&files, &options, "XM201").is_empty());
}

const WWMI_CALL: &str = "[Present]\nrun = CommandList\\WWMIv1\\BackupRestoreVertexTextureSlots\n";

fn flagged_under(profile_id: &str, text: &str, code: &str) -> Vec<String> {
    let options = LintOptions {
        profile: profile::by_id(profile_id),
        ..LintOptions::bundled()
    };
    flagged(&[("/v/loose/a.ini", text)], &options, code)
}

#[test]
fn a_library_call_resolves_under_its_own_profile_only() {
    assert!(flagged_under("wwmi", WWMI_CALL, "XM206").is_empty());
    assert!(flagged_under("wwmi", WWMI_CALL, "XM201").is_empty());

    // `WWMIv1` is not a namespace of the zzmi library, so the namespace check fires, not XM201.
    assert_eq!(
        flagged_under("zzmi", WWMI_CALL, "XM206"),
        [resolved("/v/loose/a.ini")]
    );
    assert!(flagged_under("zzmi", WWMI_CALL, "XM201").is_empty());
}

#[test]
fn a_loose_mod_calling_the_zzmi_library_resolves_it() {
    let zzmi = "[Present]\nrun = CommandList\\ZZMI\\SetTextures\n";
    for code in ["XM201", "XM206"] {
        let found = flagged(&[("/v/loose/a.ini", zzmi)], &LintOptions::bundled(), code);
        assert!(found.is_empty(), "{code}: {found:?}");
    }
}

const GLOBAL_CALL: &str = "[Present]\nrun = CommandListSkinTexture\n";

#[test]
fn a_library_global_is_accepted_when_no_mi_is_detected() {
    let options = LintOptions::bundled();
    let found = flagged(&[("/v/loose/a.ini", GLOBAL_CALL)], &options, "XM201");
    assert!(found.is_empty(), "{found:?}");

    let lower = "[Present]\nrun = commandlistskintexture\n";
    assert!(flagged(&[("/v/loose/a.ini", lower)], &options, "XM201").is_empty());
}

#[test]
fn a_chosen_profile_decides_the_library_global() {
    assert!(flagged_under("zzmi", GLOBAL_CALL, "XM201").is_empty());

    assert_eq!(
        flagged_under("xxmi", GLOBAL_CALL, "XM201"),
        [resolved("/v/loose/a.ini")]
    );
    assert_eq!(
        flagged_under("wwmi", GLOBAL_CALL, "XM201"),
        [resolved("/v/loose/a.ini")]
    );
}

#[test]
fn xxmi_toml_naming_xxmi_is_not_a_fallback() {
    let dir = scratch("toml-xxmi");
    fs::write(dir.join("xxmi.toml"), "profile = \"xxmi\"\n").unwrap();
    fs::write(dir.join("a.ini"), GLOBAL_CALL).unwrap();

    let result = lint_paths(std::slice::from_ref(&dir), &LintOptions::bundled()).unwrap();
    let flagged = result
        .files
        .iter()
        .any(|f| f.diagnostics.iter().any(|d| d.id == "XM201"));
    assert!(flagged);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unknown_names_are_still_unresolved_without_an_mi() {
    let options = LintOptions::bundled();
    let unknown = "[Present]\nrun = CommandListNoSuchThing\n";
    assert_eq!(
        flagged(&[("/v/loose/a.ini", unknown)], &options, "XM201"),
        [resolved("/v/loose/a.ini")]
    );

    // The namespace picks zzmi, so this is not a fallback.
    let namespaced = "[Present]\nrun = CommandList\\ZZMI\\NoSuchThing\n";
    assert_eq!(
        flagged(&[("/v/loose/a.ini", namespaced)], &options, "XM201"),
        [resolved("/v/loose/a.ini")]
    );
}

fn stride_mod(run: &str) -> String {
    format!(
        "[ResourcePosition]\ntype = Buffer\nstride = 40\nfilename = Position.buf\n\n\
         [TextureOverrideCall]\nhash = 0223abcd\nvb0 = ResourcePosition\nrun = {run}\n\
         override_vertex_count = 1000\noverride_byte_stride = 52\n"
    )
}

#[test]
fn xm306_looks_past_a_library_call_but_not_an_unresolved_one() {
    let options = LintOptions {
        rules: Some(vec!["XM306".to_string()]),
        profile: profile::by_id("zzmi"),
        ..LintOptions::bundled()
    };
    let flagged_run =
        |run: &str| flagged(&[("/v/loose/a.ini", &stride_mod(run))], &options, "XM306");

    // Library lists are known not to bind a vertex buffer, unlike unresolved ones.
    assert_eq!(
        flagged_run("CommandList\\ZZMI\\SetTextures"),
        [resolved("/v/loose/a.ini")]
    );
    assert!(flagged_run("CommandListMissing").is_empty());
}
