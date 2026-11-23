//! `read_source`: numbered lines of one file of a checkout.

use std::fs;
use std::path::Path;

use serde::Serialize;
use xxmi_core::encoding::decode_text;

use super::SourceError;
use super::tree::{safe_join, safe_relative};

pub const MAX_EXCERPT_LINES: usize = 400;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Excerpt {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub text: String,
}

/// Lines split like JavaScript's `split(/\r?\n/)`.
fn split_lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

pub fn read_excerpt(
    root: &Path,
    path: &str,
    start_line: usize,
    end_line: usize,
) -> Result<Excerpt, SourceError> {
    if end_line < start_line {
        return Err(SourceError("end_line must be >= start_line".into()));
    }
    if end_line - start_line + 1 > MAX_EXCERPT_LINES {
        return Err(SourceError(format!(
            "at most {MAX_EXCERPT_LINES} lines per call"
        )));
    }

    let (Some(full), Some(relative)) = (safe_join(root, path), safe_relative(path)) else {
        return Err(SourceError(format!(
            "path \"{path}\" is not inside the repository"
        )));
    };
    if !full.is_file() {
        return Err(SourceError(format!("no such file: {path}")));
    }
    let bytes = fs::read(&full).map_err(|e| SourceError(format!("cannot read {path}: {e}")))?;

    let text = decode_text(&bytes).0;
    let lines = split_lines(&text);
    let last = end_line.min(lines.len());
    let width = last.to_string().len();
    let numbered: Vec<String> = lines
        .iter()
        .enumerate()
        .skip(start_line - 1)
        .take(last.saturating_sub(start_line - 1))
        .map(|(i, line)| format!("{:>width$}| {line}", i + 1))
        .collect();

    Ok(Excerpt {
        path: relative,
        start_line,
        end_line: last,
        total_lines: lines.len(),
        text: numbered.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_lines_and_clamps_the_end() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.cpp"), "one\r\ntwo\nthree").unwrap();

        let got = read_excerpt(dir.path(), "a.cpp", 2, 10).unwrap();
        assert_eq!((got.start_line, got.end_line, got.total_lines), (2, 3, 3));
        assert_eq!(got.text, "2| two\n3| three");
    }

    #[test]
    fn rejects_bad_ranges_and_paths() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_excerpt(dir.path(), "a.cpp", 5, 2).is_err());
        assert!(read_excerpt(dir.path(), "a.cpp", 1, 500).is_err());
        assert!(read_excerpt(dir.path(), "../a.cpp", 1, 2).is_err());
        assert!(read_excerpt(dir.path(), "missing.cpp", 1, 2).is_err());
    }
}
