//! `search_source`: ripgrep over a checkout. ripgrep's regex engine runs in linear time,
//! so a hostile pattern cannot hang the server, and a deadline kills it when the tree is slow.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::SourceError;

#[derive(Debug, Serialize)]
pub struct ContextLine {
    pub line: u64,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SearchMatch {
    pub path: String,
    pub line: u64,
    pub text: String,
    pub context: Vec<ContextLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
    pub timed_out: bool,
}

pub struct SearchOptions<'a> {
    pub path_glob: Option<&'a str>,
    pub max_results: usize,
    pub timeout: Duration,
}

const MAX_OUTPUT_BYTES: usize = 8_000_000;
/// Context lines either side of a match.
const CONTEXT: u64 = 2;

fn rg_args(query: &str, path_glob: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "--json",
        "--no-config",
        "--smart-case",
        "--context",
        "2",
        "--max-filesize",
        "2M",
        "--max-columns",
        "300",
        "--regex-size-limit",
        "5M",
        "--max-count",
        "10",
        "-e",
    ]
    .map(String::from)
    .to_vec();
    args.push(query.to_string());
    if let Some(glob) = path_glob.filter(|g| !g.is_empty()) {
        args.extend(["--glob".to_string(), glob.to_string()]);
    }
    args.extend(["--".to_string(), ".".to_string()]);
    args
}

/// What ripgrep printed before it exited, or before it was cut off.
struct RgRun {
    stdout: Vec<u8>,
    stderr: String,
    exit_code: Option<i32>,
    cut_off: bool,
}

fn spawn_rg(root: &Path, args: &[String]) -> Result<Child, SourceError> {
    Command::new("rg")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => SourceError(
                "search is unavailable: ripgrep (rg) is not installed on the server".into(),
            ),
            _ => SourceError(format!("search failed to start: {e}")),
        })
}

fn collect_stderr(pipe: Option<ChildStderr>) -> JoinHandle<String> {
    tokio::spawn(async move {
        let mut text = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut text).await;
        }
        String::from_utf8_lossy(&text).into_owned()
    })
}

/// Reads until EOF, the deadline or the output cap; true when it stopped early.
async fn read_stdout(mut pipe: ChildStdout, timeout: Duration) -> (Vec<u8>, bool) {
    let mut stdout = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut chunk = vec![0u8; 64 * 1024];

    loop {
        tokio::select! {
            read = pipe.read(&mut chunk) => match read {
                Ok(0) | Err(_) => return (stdout, false),
                Ok(n) => stdout.extend_from_slice(&chunk[..n]),
            },
            () = tokio::time::sleep_until(deadline) => return (stdout, true),
        }
        if stdout.len() > MAX_OUTPUT_BYTES {
            stdout.truncate(MAX_OUTPUT_BYTES);
            return (stdout, true);
        }
    }
}

async fn run_rg(root: &Path, args: &[String], timeout: Duration) -> Result<RgRun, SourceError> {
    let mut child = spawn_rg(root, args)?;
    let stderr_task = collect_stderr(child.stderr.take());

    let (stdout, cut_off) = match child.stdout.take() {
        Some(pipe) => read_stdout(pipe, timeout).await,
        None => (Vec::new(), false),
    };
    if cut_off {
        let _ = child.kill().await;
    }

    let exit_code = child.wait().await.ok().and_then(|status| status.code());
    let stderr = stderr_task.await.unwrap_or_default();
    Ok(RgRun {
        stdout,
        stderr,
        exit_code,
        cut_off,
    })
}

pub async fn search_source(
    root: &Path,
    query: &str,
    options: &SearchOptions<'_>,
) -> Result<SearchResult, SourceError> {
    let run = run_rg(root, &rg_args(query, options.path_glob), options.timeout).await?;

    // rg exits 1 when nothing matched and 2 on a bad pattern (stderr explains).
    if !run.cut_off && run.exit_code == Some(2) && run.stdout.is_empty() {
        let first = run.stderr.split('\n').next().unwrap_or("regex error");
        return Err(SourceError(format!("invalid search: {first}")));
    }

    let stdout = String::from_utf8_lossy(&run.stdout);
    let (matches, truncated) = parse_rg(&stdout, options.max_results);
    Ok(SearchResult {
        matches,
        truncated,
        timed_out: run.cut_off,
    })
}

fn text_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.get("text")?.as_str()
}

/// `./a\b.cpp` as `a/b.cpp`.
fn clean_path(raw: &str) -> String {
    let stripped = raw
        .strip_prefix("./")
        .or_else(|| raw.strip_prefix(".\\"))
        .unwrap_or(raw);
    stripped.replace('\\', "/")
}

fn line_text(data: &Value) -> String {
    let raw = text_field(data, "lines").unwrap_or("");
    let raw = raw.strip_suffix('\n').unwrap_or(raw);
    raw.strip_suffix('\r').unwrap_or(raw).to_string()
}

/// Collects one file's matches and their surrounding lines as ripgrep reports them.
#[derive(Default)]
struct RgParser {
    matches: Vec<SearchMatch>,
    pending: Vec<ContextLine>,
    // False between the end of one file's results and the first match of the next.
    open: bool,
}

impl RgParser {
    fn add_context(&mut self, path: Option<&String>, line: u64, text: String) {
        let open = self.open;
        let after = self.matches.last_mut().filter(|last| {
            open && Some(&last.path) == path && line > last.line && line <= last.line + CONTEXT
        });
        match after {
            Some(last) => last.context.push(ContextLine { line, text }),
            None => self.pending.push(ContextLine { line, text }),
        }
    }

    fn add_match(&mut self, path: String, line: u64, text: String) {
        self.open = true;
        let before = self.pending.drain(..).filter(|c| c.line < line).collect();
        self.matches.push(SearchMatch {
            path,
            line,
            text,
            context: before,
        });
    }

    fn end_file(&mut self) {
        self.pending.clear();
        self.open = false;
    }
}

/// Matches with their context, in output order, and whether more than `max_results` exist.
fn parse_rg(stdout: &str, max_results: usize) -> (Vec<SearchMatch>, bool) {
    let mut parser = RgParser::default();

    for raw in stdout.split('\n').filter(|l| !l.is_empty()) {
        // A line cut off by the kill does not parse.
        let Ok(message) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let kind = message.get("type").and_then(Value::as_str).unwrap_or("");
        let data = message.get("data").unwrap_or(&Value::Null);
        let path = text_field(data, "path").map(clean_path);
        let line = data.get("line_number").and_then(Value::as_u64);

        match (kind, line) {
            ("context", Some(line)) => parser.add_context(path.as_ref(), line, line_text(data)),
            ("match", Some(line)) if path.is_some() => {
                if parser.matches.len() >= max_results {
                    return (parser.matches, true);
                }
                parser.add_match(path.unwrap_or_default(), line, line_text(data));
            }
            ("end", _) => parser.end_file(),
            _ => {}
        }
    }
    (parser.matches, false)
}
