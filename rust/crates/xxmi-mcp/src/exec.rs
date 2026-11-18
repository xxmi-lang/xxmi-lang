//! Process spawning for git: argument arrays only (never a shell) and a timeout on every call.
//! Children are killed when their handle drops, so a cancelled or timed out call leaves nothing
//! running.

use std::fmt;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStdout, Command};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub const MAX_OUTPUT_BYTES: usize = 16_000_000;
const MAX_STDERR_CHARS: usize = 2_000;

#[derive(Debug)]
pub struct ExecError {
    pub message: String,
    /// A timeout says nothing about the repository's health, so callers don't recover from it.
    pub timed_out: bool,
}

impl ExecError {
    fn failure(label: &str, detail: &str, timed_out: bool) -> Self {
        let detail = detail.trim();
        let skip = detail.chars().count().saturating_sub(MAX_STDERR_CHARS);
        let tail: String = detail.chars().skip(skip).collect();
        Self {
            message: format!("{label} failed: {tail}"),
            timed_out,
        }
    }

    fn timeout(call: &Call<'_>) -> Self {
        let detail = format!("timed out after {} ms", call.timeout.as_millis());
        Self::failure(call.label, &detail, true)
    }
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExecError {}

/// The git executable, optionally followed by arguments that go before every call's own.
#[derive(Clone, Debug)]
pub struct GitCommand {
    program: String,
    prefix: Vec<String>,
}

impl Default for GitCommand {
    fn default() -> Self {
        Self::new("git", &[])
    }
}

pub struct Call<'a> {
    pub cwd: Option<&'a Path>,
    pub timeout: Duration,
    /// Names the step in error messages, e.g. `git fetch`.
    pub label: &'a str,
}

impl GitCommand {
    pub fn new(program: &str, prefix: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            prefix: prefix.iter().map(|arg| (*arg).to_string()).collect(),
        }
    }

    /// No prompts (a missing repo must fail, not hang). `safe.directory` is set through the
    /// environment because the volume may belong to another user than the one running git. No
    /// background gc or maintenance: a detached child would outlive the call and race a cache
    /// removal.
    fn spawn(&self, args: &[&str], call: &Call<'_>) -> Result<Child, ExecError> {
        let mut command = Command::new(&self.program);
        command
            .args(&self.prefix)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_COUNT", "4")
            .env("GIT_CONFIG_KEY_0", "safe.directory")
            .env("GIT_CONFIG_VALUE_0", "*")
            .env("GIT_CONFIG_KEY_1", "gc.auto")
            .env("GIT_CONFIG_VALUE_1", "0")
            .env("GIT_CONFIG_KEY_2", "maintenance.auto")
            .env("GIT_CONFIG_VALUE_2", "false")
            .env("GIT_CONFIG_KEY_3", "fetch.writeCommitGraph")
            .env("GIT_CONFIG_VALUE_3", "false")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = call.cwd {
            command.current_dir(cwd);
        }
        command
            .spawn()
            .map_err(|e| ExecError::failure(call.label, &e.to_string(), false))
    }

    /// Runs git and resolves with its stdout.
    pub async fn run(&self, args: &[&str], call: &Call<'_>) -> Result<String, ExecError> {
        let mut child = self.spawn(args, call)?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let work = async {
            let (out, err, status) = tokio::join!(
                read_limited(stdout, MAX_OUTPUT_BYTES + 1),
                read_all(stderr),
                child.wait()
            );
            (out, err, status)
        };
        let Ok((out, err, status)) = timeout(call.timeout, work).await else {
            return Err(ExecError::timeout(call));
        };

        if out.len() > MAX_OUTPUT_BYTES {
            let detail = format!("output exceeded {MAX_OUTPUT_BYTES} bytes");
            return Err(ExecError::failure(call.label, &detail, false));
        }
        match status {
            Ok(status) if status.success() => Ok(String::from_utf8_lossy(&out).into_owned()),
            Ok(status) => Err(failed_exit(call, &err, status.code())),
            Err(e) => Err(ExecError::failure(call.label, &e.to_string(), false)),
        }
    }

    /// Starts git with its stdout left to the caller, for output too big to buffer.
    pub fn stream(&self, args: &[&str], call: &Call<'_>) -> Result<Streamed, ExecError> {
        let mut child = self.spawn(args, call)?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take();
        Ok(Streamed {
            child,
            stdout: Some(stdout),
            stderr: tokio::spawn(async move {
                String::from_utf8_lossy(&read_all(stderr).await).into_owned()
            }),
            label: call.label.to_string(),
        })
    }
}

fn failed_exit(call: &Call<'_>, stderr: &[u8], code: Option<i32>) -> ExecError {
    let text = String::from_utf8_lossy(stderr);
    let detail = if text.trim().is_empty() {
        format!("exit {}", code.map_or("?".to_string(), |c| c.to_string()))
    } else {
        text.into_owned()
    };
    ExecError::failure(call.label, &detail, false)
}

async fn read_all<R: AsyncRead + Unpin>(reader: Option<R>) -> Vec<u8> {
    read_limited(reader, MAX_OUTPUT_BYTES).await
}

async fn read_limited<R: AsyncRead + Unpin>(reader: Option<R>, limit: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(reader) = reader {
        // A read error ends the stream like EOF; the exit status carries the failure.
        let _ = reader.take(limit as u64).read_to_end(&mut bytes).await;
    }
    bytes
}

/// A running git whose stdout the caller consumes.
pub struct Streamed {
    child: Child,
    stdout: Option<ChildStdout>,
    stderr: JoinHandle<String>,
    label: String,
}

impl Streamed {
    pub fn take_stdout(&mut self) -> ChildStdout {
        self.stdout.take().expect("stdout is taken once")
    }

    /// Waits for the process; a non-zero status is an error carrying its stderr.
    pub async fn finish(&mut self) -> Result<(), ExecError> {
        let status = self
            .child
            .wait()
            .await
            .map_err(|e| ExecError::failure(&self.label, &e.to_string(), false))?;
        let stderr = (&mut self.stderr).await.unwrap_or_default();
        if status.success() {
            return Ok(());
        }
        let detail = if stderr.trim().is_empty() {
            format!(
                "exit {}",
                status.code().map_or("?".to_string(), |c| c.to_string())
            )
        } else {
            stderr
        };
        Err(ExecError::failure(&self.label, &detail, false))
    }

    /// After the consumer gave up: git's own explanation if it ends by itself within `grace`
    /// (the stream often ends because git failed), else nothing; the process is killed either way.
    pub async fn explain(&mut self, grace: Duration) -> Option<ExecError> {
        let exited = timeout(grace, self.finish()).await;
        let _ = self.child.start_kill();
        match exited {
            Ok(Err(error)) if !error.message.ends_with("exit ?") => Some(error),
            _ => None,
        }
    }
}
