//! In-process timings on one document: full parse, incremental re-parse (a single-character
//! change at varied positions) and lint.
//!
//!   node rust/bench/lsp-latency.mjs --write-doc big.ini
//!   cargo run --release -p xxmi-core --example timing -- big.ini

use std::time::{Duration, Instant};

use xxmi_core::diagnostic::{Position, Range};
use xxmi_core::document::{Document, TextChange};
use xxmi_core::lint::lint;

const EDITS: usize = 200;

fn main() {
    let path = std::env::args().nth(1).expect("usage: timing <ini file>");
    let text = std::fs::read_to_string(path).expect("the document is readable");

    let parse = repeat(20, || drop(Document::new(text.clone())));
    let mut doc = Document::new(text.clone());
    let lint_time = repeat(20, || drop(lint(&doc, None)));
    let edit = edit_times(&mut doc);

    println!("lines: {}", text.lines().count());
    println!("full parse (scan + every section tree): {}", median(parse));
    println!("lint of the parsed document: {}", median(lint_time));
    println!(
        "incremental edit (apply_changes): median {}, max {}",
        median(edit.clone()),
        max(edit)
    );
}

fn repeat(times: usize, mut run: impl FnMut()) -> Vec<Duration> {
    (0..times)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed()
        })
        .collect()
}

fn edit_times(doc: &mut Document) -> Vec<Duration> {
    let line_count = doc.text().lines().count() as u32;
    let mut seed: u64 = 12345;

    (0..EDITS)
        .map(|_| {
            seed = (seed * 1_103_515_245 + 12345) % 2_147_483_648;
            let at = Position {
                line: (seed % u64::from(line_count)) as u32,
                character: (seed % 7) as u32,
            };
            let change = TextChange {
                range: Some(Range { start: at, end: at }),
                text: "x".to_string(),
            };

            let start = Instant::now();
            doc.apply_changes(&[change]);
            start.elapsed()
        })
        .collect()
}

fn median(mut times: Vec<Duration>) -> String {
    times.sort();
    format!("{:.3} ms", times[times.len() / 2].as_secs_f64() * 1000.0)
}

fn max(times: Vec<Duration>) -> String {
    format!(
        "{:.3} ms",
        times.iter().max().expect("not empty").as_secs_f64() * 1000.0
    )
}
