#!/usr/bin/env node
// Parity check between the TypeScript spec extractor and the Rust one: extracts the XXMI DLL at
// master and a few release tags (git archive into temp dirs) with both, and compares every
// spec/generated/*.json byte for byte. For master it also compares the Rust spec merged with
// spec/overlay (--full) against packages/core/dist/spec.json. Prints time and peak memory of both
// extractors at master. Exits 1 on any difference.
//
//   cargo build --release --manifest-path rust/Cargo.toml && pnpm build && node rust/bench/spec-parity.mjs
//
// XXMI_RS_EXTRACT overrides the Rust binary, XXMI_DLL_SRC and MIGOTO_DLL_SRC the checkouts.

import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir, platform } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const rustBinary = process.env.XXMI_RS_EXTRACT ?? join(root, 'rust/target/release/xxmi-spec-extract');
const dllRepo = resolve(root, process.env.XXMI_DLL_SRC ?? '../xxmi-refs/XXMI-Libs-Package');
const baselineRepo = resolve(root, process.env.MIGOTO_DLL_SRC ?? '../xxmi-refs/3Dmigoto');
const refs = ['master', 'v1.3.3', 'v1.3.2', 'v1.3.1', 'v1.2.3', 'v1.2.2', 'v1.2.0'];

const scratch = mkdtempSync(join(tmpdir(), 'spec-parity-'));
let failures = 0;

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 1 << 28, ...options });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} exited ${result.status}: ${result.stderr}`);
  }
  return result;
}

function gitInfo(repo, ref) {
  const [commit, commitDate] = run('git', ['-C', repo, 'log', '-1', '--format=%H%n%cI', ref])
    .stdout.trim()
    .split('\n');
  const url = run('git', ['-C', repo, 'remote', 'get-url', 'origin']).stdout.trim();
  const repository = url.replace(/\.git$/, '').replace(/^git@github\.com:/, 'https://github.com/');
  return { repository, commit, commitDate };
}

function archive(repo, ref, dir) {
  mkdirSync(dir, { recursive: true });
  const tar = run('git', ['-C', repo, 'archive', ref], { encoding: 'buffer' }).stdout;
  run('tar', ['-x', '-C', dir], { input: tar });
}

/** Wall time and peak resident memory (MB) of a command, via the platform's time(1). */
function measured(command, args) {
  const mac = platform() === 'darwin';
  const started = performance.now();
  const result = spawnSync('/usr/bin/time', [mac ? '-l' : '-v', command, ...args], {
    encoding: 'utf8',
    maxBuffer: 1 << 28,
  });
  const seconds = (performance.now() - started) / 1000;
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}: ${result.stderr}`);
  const peak = mac
    ? Number(/(\d+)\s+maximum resident set size/.exec(result.stderr)?.[1]) / 1e6
    : Number(/Maximum resident set size \(kbytes\): (\d+)/.exec(result.stderr)?.[1]) / 1e3;
  return { seconds, peak };
}

function compareDirs(label, tsDir, rustDir) {
  const names = readdirSync(tsDir).sort();
  const same = JSON.stringify(names) === JSON.stringify(readdirSync(rustDir).sort());
  let differences = same ? 0 : 1;
  for (const name of names) {
    if (readFileSync(join(tsDir, name), 'utf8') !== readFileSync(join(rustDir, name), 'utf8')) {
      console.error(`${label}: ${name} differs`);
      differences++;
    }
  }
  return differences;
}

function checkRef(ref, baselineGit) {
  const dir = join(scratch, ref);
  const source = join(dir, 'source');
  archive(dllRepo, ref, source);
  const dllGit = gitInfo(dllRepo, ref);
  const tsOut = join(dir, 'ts');
  const rustOut = join(dir, 'rust');
  const full = join(dir, 'full.json');

  const ts = measured('node', [
    join(root, 'rust/bench/spec-extract-ts.mjs'),
    source,
    baselineRepo,
    tsOut,
    JSON.stringify(dllGit),
    JSON.stringify(baselineGit),
  ]);
  const rustArgs = [
    '--dll', source,
    '--baseline', baselineRepo,
    '--out', rustOut,
    '--full', full,
    '--spec-root', join(root, 'spec'),
    '--dll-commit', dllGit.commit,
    '--dll-commit-date', dllGit.commitDate,
    '--dll-repository', dllGit.repository,
    '--baseline-commit', baselineGit.commit,
    '--baseline-commit-date', baselineGit.commitDate,
    '--baseline-repository', baselineGit.repository,
  ];
  const rust = measured(rustBinary, rustArgs);

  let differences = compareDirs(ref, tsOut, rustOut);
  if (ref === 'master') {
    const bundled = readFileSync(join(root, 'packages/core/dist/spec.json'), 'utf8');
    if (readFileSync(full, 'utf8') !== bundled) {
      console.error('master: merged spec differs from packages/core/dist/spec.json');
      differences++;
    }
    const fmt = ({ seconds, peak }) => `${seconds.toFixed(2)} s, ${peak.toFixed(0)} MB peak`;
    console.log(`master  TS: ${fmt(ts)}   Rust: ${fmt(rust)}`);
  }
  console.log(`${ref}: ${differences === 0 ? 'identical' : `${differences} differences`}`);
  return differences;
}

try {
  const baselineGit = gitInfo(baselineRepo, 'HEAD');
  for (const ref of refs) failures += checkRef(ref, baselineGit);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
process.exit(failures > 0 ? 1 : 0);
