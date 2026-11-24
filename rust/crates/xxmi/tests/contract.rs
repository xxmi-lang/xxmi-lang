//! The CLI contract bundling apps rely on: exit codes, version output and `capabilities --json`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;
use xxmi_core::rules::RULES;

fn xxmi(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xxmi"))
        .args(args)
        .output()
        .expect("the binary starts")
}

fn code(args: &[&str]) -> i32 {
    xxmi(args).status.code().expect("exited normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// A scratch directory holding one file per name, removed on drop.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("xxmi-contract-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (file, text) in files {
            fs::write(dir.join(file), text).unwrap();
        }
        Self { dir }
    }

    fn path(&self, file: &str) -> String {
        self.dir.join(file).to_string_lossy().into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn usage_errors_exit_2() {
    assert_eq!(code(&[]), 2);
    assert_eq!(code(&["no-such-command"]), 2);
    assert_eq!(code(&["lint", "--format", "yaml"]), 2);
}

#[test]
fn version_is_a_bare_semver() {
    for flag in ["--version", "-v"] {
        let output = xxmi(&[flag]);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(stdout(&output), format!("{}\n", env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn help_exits_0() {
    for args in [
        &["--help"][..],
        &["-h"],
        &["lsp", "--help"],
        &["lint", "--help"],
    ] {
        assert_eq!(code(args), 0, "{args:?}");
    }
}

#[test]
fn lint_exit_codes() {
    let fixture = Fixture::new(
        "lint",
        &[
            ("clean.ini", "[Constants]\nglobal $a = 1\n"),
            ("error.ini", "[Bogus]\n"),
            ("warning.ini", "[Resource\\R]\nbogus_key = 1\n"),
        ],
    );

    assert_eq!(
        code(&["lint", "--format", "json", &fixture.path("clean.ini")]),
        0
    );
    assert_eq!(
        code(&["lint", "--format", "json", &fixture.path("error.ini")]),
        1
    );

    let warning = fixture.path("warning.ini");
    assert_eq!(code(&["lint", &warning]), 0);
    assert_eq!(code(&["lint", "--max-warnings", "0", &warning]), 1);
    assert_eq!(code(&["lint", "--max-warnings", "5", &warning]), 0);
}

/// A symlink on Unix: copying leaves a write fd open that parallel tests' forks inherit,
/// so exec of the copy fails with ETXTBSY. Windows has no such race.
fn link_binary(link: &std::path::Path) {
    let binary = env!("CARGO_BIN_EXE_xxmi");
    #[cfg(unix)]
    std::os::unix::fs::symlink(binary, link).unwrap();
    #[cfg(windows)]
    fs::copy(binary, link).map(drop).unwrap();
}

#[test]
fn mi_package_forces_the_profile() {
    let fixture = Fixture::new(
        "mi-package",
        &[(
            "mod.ini",
            "[Present]\nrun = CommandList\\WWMIv1\\BackupRestoreVertexTextureSlots\n",
        )],
    );
    let lint = |id: &str| {
        xxmi(&[
            "lint",
            "--format",
            "json",
            "--mi-package",
            id,
            &fixture.path("mod.ini"),
        ])
    };

    let wwmi = lint("wwmi");
    let zzmi = lint("zzmi");

    assert!(!stdout(&wwmi).contains("XM20"), "{}", stdout(&wwmi));
    assert!(stdout(&zzmi).contains("XM206"), "{}", stdout(&zzmi));
}

#[test]
fn an_unknown_mi_package_is_a_usage_error_listing_the_ids() {
    let fixture = Fixture::new("mi-package-unknown", &[("mod.ini", "[Constants]\n")]);

    let output = xxmi(&["lint", "--mi-package", "nope", &fixture.path("mod.ini")]);
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert_eq!(output.status.code(), Some(2));
    for id in ["xxmi", "gimi", "srmi", "wwmi", "zzmi", "himi", "efmi"] {
        assert!(stderr.contains(id), "{stderr}");
    }
}

#[test]
fn standalone_lsp_name_answers_version() {
    let dir = Fixture::new("lsp-name", &[]);
    let link = dir.dir.join(if cfg!(windows) {
        "xxmi-lsp.exe"
    } else {
        "xxmi-lsp"
    });
    link_binary(&link);

    let output = Command::new(&link).arg("--version").output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), format!("{}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn capabilities_json_lists_the_rules_in_order() {
    let output = xxmi(&["capabilities", "--json"]);
    assert_eq!(output.status.code(), Some(0));

    let text = stdout(&output);
    assert!(text.ends_with('\n') && !text[..text.len() - 1].contains('\n'));
    let document: Value = serde_json::from_str(&text).expect("valid JSON");

    assert_eq!(document["version"], env!("CARGO_PKG_VERSION"));
    let ids: Vec<&str> = document["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    let expected: Vec<&str> = RULES.iter().map(|r| r.id).collect();
    assert_eq!(ids, expected);
}

#[test]
fn capabilities_without_json_exits_0() {
    let output = xxmi(&["capabilities"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).starts_with("XM001  error"));
}
