/**
 * Process setup for Node entry points (CLI, LSP).
 *
 * Under Node, web-tree-sitter leaks native memory on every parse once V8 tiers its WASM up to the
 * optimizing compiler: about 0.2 MB per parse even when every tree is deleted (measured with
 * Node 24 and web-tree-sitter 0.26.13 and 0.27.0; Bun doesn't leak). With V8's baseline compiler
 * only (`--liftoff-only`) memory stays flat and parsing is no slower for our grammar
 * (scripts/perf-lsp.ts). The flag can't be set after startup, so entry points relaunch
 * themselves with it.
 */
import { spawn } from 'node:child_process';

const FLAG = '--liftoff-only';
/** Set in the relaunched child, and by users who manage flags themselves. */
const SKIP_ENV = 'XXMI_NO_RELAUNCH';

/**
 * Relaunches the current Node process with `--liftoff-only` unless it already has it, it isn't
 * Node, or `XXMI_NO_RELAUNCH` is set. Resolves to true if this process relaunched and should
 * do nothing else (the child's exit code becomes this process's exit code).
 */
export function relaunchWithWasmFlags(): Promise<boolean> {
  const isBun = 'bun' in process.versions;
  if (isBun || process.env[SKIP_ENV] || process.execArgv.includes(FLAG))
    return Promise.resolve(false);
  const [, script, ...args] = process.argv;
  if (!script) return Promise.resolve(false);
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [...process.execArgv, FLAG, script, ...args], {
      stdio: 'inherit',
      env: { ...process.env, [SKIP_ENV]: '1' },
    });
    for (const signal of ['SIGINT', 'SIGTERM'] as const) {
      process.on(signal, () => {
        child.kill(signal);
      });
    }
    child.on('error', () => {
      resolve(false); // fall back to running in this process
    });
    child.on('exit', (code, signal) => {
      process.exitCode = code ?? (signal ? 1 : 0);
      resolve(true);
    });
  });
}
