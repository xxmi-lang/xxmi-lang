//! Editor features and lint must answer, never panic, for any text and any position: the release
//! build aborts on a panic, which would end the language server's session.

use std::fs;
use std::path::{Path, PathBuf};

use xxmi_core::diagnostic::Position;
use xxmi_core::lint::{ConfigLoader, LintOptions, lint_file};
use xxmi_core::navigation::{
    completions, definition, hover, outline, references, rename, target_at,
};
use xxmi_core::workspace::Workspace;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures")
}

fn ini_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            ini_files(&path, found);
        } else if path.extension().is_some_and(|e| e == "ini") {
            found.push(path);
        }
    }
}

/// Runs every position-based feature at positions in, on the edge of and past the text.
fn exercise(text: &str) {
    let options = LintOptions::bundled();
    let mut workspace = Workspace::new(options.lookup, options.snapshots);
    let path = Path::new("/xxmi-robustness/Mods/a.ini");
    workspace.set_overlay(path, text.to_string());
    let Ok(id) = workspace.add_target(path, path.parent().unwrap()) else {
        return;
    };

    let _ = lint_file(id, &workspace, &options, &ConfigLoader::default());
    let _ = outline(&workspace.files()[id]);

    let lines: Vec<&str> = text.split('\n').collect();
    for line in 0..lines.len() as u32 + 2 {
        let width = lines
            .get(line as usize)
            .map_or(0, |l| l.encode_utf16().count()) as u32;
        for character in (0..width + 3).chain([u32::MAX]) {
            probe(&workspace, id, Position { line, character });
        }
    }
    probe(
        &workspace,
        id,
        Position {
            line: u32::MAX,
            character: u32::MAX,
        },
    );
}

fn probe(workspace: &Workspace, id: usize, pos: Position) {
    let _ = hover(workspace, id, pos);
    let _ = completions(workspace, id, pos);
    let _ = rename(workspace, id, pos, None);
    let _ = rename(workspace, id, pos, Some("Renamed"));

    if let Some(target) = target_at(workspace, id, pos) {
        let _ = definition(workspace, &target);
        let _ = references(workspace, &target, true);
    }
}

fn boundaries(text: &str, step: usize) -> impl Iterator<Item = usize> + '_ {
    (0..=text.len())
        .step_by(step)
        .filter(|&at| text.is_char_boundary(at))
}

#[test]
fn degenerate_texts() {
    let texts = [
        "",
        "\u{feff}",
        "\n",
        "\r\n",
        "[",
        "[]",
        "=",
        "$",
        "run = CommandList\\Foo",
        "[CommandListA]\nrun = CommandList\\Foo",
        "[A]\nk = \u{1F600}\u{1F600}\nrun = \u{fffd}",
        "[A]\r\nrun = $\\a\\b\r\nif $x ==",
    ];
    for text in texts {
        exercise(text);
    }
}

#[test]
fn long_single_line() {
    let text = format!("[A]\nk = {}", "x".repeat(200_000));
    let options = LintOptions::bundled();
    let mut workspace = Workspace::new(options.lookup, options.snapshots);
    let path = Path::new("/xxmi-robustness/Mods/long.ini");
    workspace.set_overlay(path, text);
    let id = workspace.add_target(path, path.parent().unwrap()).unwrap();

    let _ = lint_file(id, &workspace, &options, &ConfigLoader::default());
    for character in [0, 5, 100_000, 200_004, 200_005, 300_000] {
        probe(&workspace, id, Position { line: 1, character });
    }
}

#[test]
fn every_fixture_and_its_truncations() {
    let mut files = Vec::new();
    ini_files(&fixtures(), &mut files);

    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        exercise(&text);

        let step = (text.len() / 40).max(1);
        for end in boundaries(&text, step) {
            let (cut, _) = text.split_at(end);
            exercise(cut);
        }
    }
}
