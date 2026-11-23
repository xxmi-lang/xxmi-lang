/**
 * Builds the single-file `xxmi` binary with cargo (docs/rust-port.md). The language server is the
 * same binary: a copy named `xxmi-lsp` serves LSP (rust/crates/xxmi/src/main.rs).
 *
 *   pnpm build:binaries [--target host | <os>-<arch>[,…]] [--out dist/bin]
 *
 * `--target host` builds for this machine and writes dist/bin/xxmi[.exe], dist/bin/xxmi-lsp[.exe]
 * and dist/bin/xxmi-<os>-<arch>[.exe], the name the VS Code packaging reads.
 * `--target <os>-<arch>` writes dist/bin/xxmi-<os>-<arch>[.exe], the names the release reads.
 * Run `--help` for the full list.
 */
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { OS_BY_NODE_PLATFORM, RELEASE_TARGETS, targetName } from './platforms.ts';

/** Release target -> Rust target triple. */
export const TARGETS: Record<string, string> = Object.fromEntries(
  RELEASE_TARGETS.map((t) => [targetName(t), t.triple]),
);

const USAGE = `Usage: pnpm build:binaries [--target <list>] [--out dir]

  --target host          this machine: dist/bin/xxmi, xxmi-lsp and xxmi-<os>-<arch>[.exe]
  --target <os>-<arch>   one release target: dist/bin/xxmi-<os>-<arch>[.exe]
  (no --target)          every release target. Needs a cross toolchain for each (rustup target add);
                         CI builds one runner per target,
                         except macOS: both darwin targets on one runner (release.yml).

Targets: ${Object.keys(TARGETS).join(', ')}
`;

const REPO = resolve(import.meta.dirname, '..');
const { values } = parseArgs({
  options: {
    target: { type: 'string' },
    out: { type: 'string' },
    help: { type: 'boolean', short: 'h' },
  },
});
if (values.help) {
  console.log(USAGE);
  process.exit(0);
}
const out = resolve(values.out ?? join(REPO, 'dist/bin'));
const targets = values.target?.split(',') ?? Object.keys(TARGETS);

/** The Rust triple of a release target; undefined for host (this machine). */
function tripleOf(target: string): string | undefined {
  if (target === 'host') return undefined;
  const triple = TARGETS[target];
  if (!triple) {
    throw new Error(
      `unknown target ${target}; expected host or one of ${Object.keys(TARGETS).join(', ')}`,
    );
  }
  return triple;
}

/** The release name of this machine, e.g. macos-arm64. */
function hostName(): string {
  const os = OS_BY_NODE_PLATFORM[process.platform];
  if (!os || (process.arch !== 'x64' && process.arch !== 'arm64')) {
    throw new Error(`no release name for ${process.platform}-${process.arch}`);
  }
  return targetName({ os, arch: process.arch });
}

/** cargo's output for one target; `triple` undefined means this machine. */
function cargoBuild(triple: string | undefined, exe: string): string {
  const args = [
    'build',
    '--release',
    '--manifest-path',
    join(REPO, 'rust/Cargo.toml'),
    '-p',
    'xxmi',
    ...(triple ? ['--target', triple] : []),
  ];
  console.log(`cargo ${args.join(' ')}`);
  execFileSync('cargo', args, { stdio: 'inherit', cwd: REPO });
  return join(REPO, 'rust/target', ...(triple ? [triple] : []), 'release', `xxmi${exe}`);
}

function build(target: string): void {
  const triple = tripleOf(target);
  const host = target === 'host';
  const windows = host ? process.platform === 'win32' : target.startsWith('windows-');
  const exe = windows ? '.exe' : '';
  const built = cargoBuild(triple, exe);

  const names = host
    ? [`xxmi${exe}`, `xxmi-lsp${exe}`, `xxmi-${hostName()}${exe}`]
    : [`xxmi-${target}${exe}`];
  for (const name of names) {
    const dest = join(out, name);
    // macOS kills a binary overwritten in place once it has run, so replace the file instead.
    rmSync(dest, { force: true });
    copyFileSync(built, dest);
    chmodSync(dest, 0o755);
    console.log(`wrote ${dest}`);
  }
}

// Checks every name before the slow part starts.
targets.forEach(tripleOf);

mkdirSync(out, { recursive: true });

for (const target of targets) build(target);
