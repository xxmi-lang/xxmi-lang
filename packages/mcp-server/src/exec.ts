/**
 * Process spawning for git: argument arrays only (never a shell) and a timeout on every call.
 * Errors carry the child's stderr.
 */
import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { PassThrough, type Readable } from 'node:stream';

export class ExecError extends Error {
  /** A timeout says nothing about the repository's health, so callers don't recover from it. */
  readonly timedOut: boolean;

  constructor(message: string, timedOut = false) {
    super(message);
    this.timedOut = timedOut;
  }
}

export interface ExecOptions {
  cwd?: string;
  timeoutMs: number;
  /** Names the step in error messages, e.g. `git fetch`. */
  label: string;
}

export const MAX_OUTPUT_BYTES = 16_000_000;
const MAX_STDERR_CHARS = 2_000;

/**
 * No prompts (a missing repo must fail, not hang). `safe.directory` is set through the
 * environment because the volume may belong to another user than the one running git.
 * No background gc or maintenance: a detached child would outlive the call and race a cache removal.
 */
function gitEnv(): NodeJS.ProcessEnv {
  return {
    ...process.env,
    GIT_TERMINAL_PROMPT: '0',
    GIT_CONFIG_COUNT: '4',
    GIT_CONFIG_KEY_0: 'safe.directory',
    GIT_CONFIG_VALUE_0: '*',
    GIT_CONFIG_KEY_1: 'gc.auto',
    GIT_CONFIG_VALUE_1: '0',
    GIT_CONFIG_KEY_2: 'maintenance.auto',
    GIT_CONFIG_VALUE_2: 'false',
    GIT_CONFIG_KEY_3: 'fetch.writeCommitGraph',
    GIT_CONFIG_VALUE_3: 'false',
  };
}

function failure(label: string, detail: string, timedOut = false): ExecError {
  return new ExecError(`${label} failed: ${detail.trim().slice(-MAX_STDERR_CHARS)}`, timedOut);
}

const timeoutFailure = (o: ExecOptions): ExecError =>
  failure(o.label, `timed out after ${o.timeoutMs} ms`, true);

/** Node reports a timeout and an exceeded `maxBuffer` alike as `killed`; the code tells them apart. */
function runFailure(error: Error, stderr: string, o: ExecOptions): ExecError {
  if ('code' in error && error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER') {
    return failure(o.label, `output exceeded ${MAX_OUTPUT_BYTES} bytes`);
  }
  if ('killed' in error && error.killed) return timeoutFailure(o);
  return failure(o.label, stderr || error.message);
}

/** Runs `command` and resolves with its stdout. */
export function run(command: string, args: readonly string[], o: ExecOptions): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(
      command,
      args,
      {
        cwd: o.cwd,
        env: gitEnv(),
        timeout: o.timeoutMs,
        maxBuffer: MAX_OUTPUT_BYTES,
        killSignal: 'SIGKILL',
        encoding: 'utf8',
      },
      (error, stdout, stderr) => {
        if (error) reject(runFailure(error, stderr, o));
        else resolve(stdout);
      },
    );
  });
}

export interface StreamedCommand {
  stdout: Readable;
  /** Settles when the process ended; rejects on a non-zero status, a timeout or `kill()`. */
  exit: Promise<void>;
  /** True once `kill()` was called: the rejection of `exit` is then ours, not the command's. */
  readonly killed: boolean;
  kill(): void;
}

interface StreamState {
  stderr: string;
  killed: boolean;
  timedOut: boolean;
}

function collectStderr(child: ChildProcess, state: StreamState): void {
  child.stderr?.setEncoding('utf8');
  child.stderr?.on('data', (chunk: string) => {
    state.stderr = (state.stderr + chunk).slice(-MAX_STDERR_CHARS);
  });
}

function startTimeout(child: ChildProcess, state: StreamState, ms: number): NodeJS.Timeout {
  return setTimeout(() => {
    state.timedOut = true;
    child.kill('SIGKILL');
  }, ms);
}

function exitPromise(
  child: ChildProcess,
  state: StreamState,
  timer: NodeJS.Timeout,
  o: ExecOptions,
): Promise<void> {
  const exit = new Promise<void>((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code) => {
      clearTimeout(timer);
      if (state.timedOut) reject(timeoutFailure(o));
      else if (state.killed) reject(failure(o.label, 'killed'));
      else if (code === 0) resolve();
      else reject(failure(o.label, state.stderr || `exit ${code ?? '?'}`));
    });
  });
  // A caller that stopped caring (extraction failed first) must not trigger an unhandled rejection.
  exit.catch(() => undefined);
  return exit;
}

/** Starts `command` and exposes its stdout as a stream, for output too big to buffer. */
export function stream(command: string, args: readonly string[], o: ExecOptions): StreamedCommand {
  const child = spawn(command, args, {
    cwd: o.cwd,
    env: gitEnv(),
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const state: StreamState = { stderr: '', killed: false, timedOut: false };
  collectStderr(child, state);
  const timer = startTimeout(child, state, o.timeoutMs);
  // Node drains a stdout nobody reads once the child exits, so a consumer that attaches a tick
  // late (after an `await`) would find it empty; piping at once keeps the data.
  const stdout = child.stdout.pipe(new PassThrough());

  return {
    stdout,
    exit: exitPromise(child, state, timer, o),
    get killed() {
      return state.killed;
    },
    kill() {
      // A process that already ended on its own keeps its own failure as the explanation.
      if (child.exitCode === null && child.signalCode === null) state.killed = true;
      child.kill('SIGKILL');
    },
  };
}
