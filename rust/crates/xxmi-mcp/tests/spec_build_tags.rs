//! Opt-in: builds with the server's own path from real DLL tags, as the server does per version.
//! `XXMI_DLL_SRC` is a XXMI-Libs-Package git checkout, `MIGOTO_DLL_SRC` a 3Dmigoto one.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use xxmi_mcp::spec_build::{BuildInput, BuiltSpec, SourceInput, build_version_spec};

const TAGS: [&str; 7] = [
    "v1.2.0", "v1.2.1", "v1.2.2", "v1.2.3", "v1.3.0", "v1.3.1", "v1.3.2",
];

fn sources() -> Option<(PathBuf, PathBuf)> {
    let dll = std::env::var("XXMI_DLL_SRC").ok()?;
    let migoto = std::env::var("MIGOTO_DLL_SRC").ok()?;
    Some((dll.into(), migoto.into()))
}

fn git(repo: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?} failed");
    out.stdout
}

fn materialize(repo: &Path, tag: &str, into: &Path) -> SourceInput {
    let archive = git(repo, &["archive", tag]);
    let mut tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(into)
        .stdin(Stdio::piped())
        .spawn()
        .expect("tar runs");
    std::io::Write::write_all(&mut tar.stdin.take().unwrap(), &archive).unwrap();
    assert!(tar.wait().unwrap().success());

    let commit = String::from_utf8(git(repo, &["rev-parse", tag])).unwrap();
    SourceInput {
        dir: into.to_path_buf(),
        commit: commit.trim().to_string(),
        commit_date: String::new(),
        repository: String::new(),
    }
}

fn build(dll: &Path, migoto: &Path, tag: &str) -> BuiltSpec {
    let dir = tempfile::tempdir().unwrap();
    let input = BuildInput {
        dll: materialize(dll, tag, dir.path()),
        baseline: SourceInput {
            dir: migoto.to_path_buf(),
            commit: "b".repeat(40),
            commit_date: String::new(),
            repository: String::new(),
        },
    };
    build_version_spec(&input).expect("the spec builds")
}

fn overlay_warnings(built: &BuiltSpec) -> Vec<&String> {
    built
        .warnings
        .iter()
        .filter(|w| w.starts_with("overlay:"))
        .collect()
}

#[test]
fn every_tag_builds_and_missing_overlay_entries_are_warnings() {
    let Some((dll, migoto)) = sources() else {
        return;
    };
    for tag in TAGS {
        let built = build(&dll, &migoto, tag);

        let commands = built.spec["commands"].as_array().expect("commands");
        assert!(!commands.is_empty(), "{tag}");
        assert!(
            overlay_warnings(&built)
                .iter()
                .all(|w| w.contains("not in the generated spec")),
            "{tag}: {:?}",
            built.warnings
        );
    }
}

#[test]
fn every_overlay_entry_merges_into_v1_3_2() {
    let Some((dll, migoto)) = sources() else {
        return;
    };

    let built = build(&dll, &migoto, "v1.3.2");

    assert!(overlay_warnings(&built).is_empty(), "{:?}", built.warnings);
}

#[test]
fn the_1_3_2_names_are_skipped_in_v1_3_1() {
    let Some((dll, migoto)) = sources() else {
        return;
    };

    let built = build(&dll, &migoto, "v1.3.1");

    assert!(
        built
            .warnings
            .iter()
            .any(|w| w.contains("commands.snapshotinitialtextures"))
    );
}
