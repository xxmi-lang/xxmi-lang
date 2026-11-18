#!/usr/bin/env node
// Parity check between the TypeScript CLI and the Rust binary: runs `xxmi lint --format json` with
// both on fixtures/ and on a ZZMI package, and compares every diagnostic (id, severity, message,
// range, fix). Exits 1 on any mismatch.
//
//   cargo build --release --manifest-path rust/Cargo.toml && node rust/bench/parity.mjs
//
// XXMI_RS overrides the Rust binary, XXMI_PACKAGE_SRC the ZZMI folder, PARITY_REPORT writes the
// full mismatch list to a file.

import { spawnSync } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const rustBinary = process.env.XXMI_RS ?? join(root, 'rust/target/release/xxmi');
const zzmi = resolve(root, process.env.XXMI_PACKAGE_SRC ?? '../xxmi-refs/ZZMI-Package/ZZMI');
const targets = [join(root, 'fixtures'), zzmi].filter((target) => {
  const found = existsSync(target);
  if (!found) console.error(`skipping ${target}: not found`);
  return found;
});

const commands = {
  ts: ['node', ['--conditions=source', join(root, 'packages/cli/src/cli.ts')]],
  rust: [rustBinary, []],
};

function lint(which, target) {
  const [command, prefix] = commands[which];
  const started = performance.now();
  const run = spawnSync(command, [...prefix, 'lint', '--format', 'json', target], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 1 << 30,
  });
  const seconds = (performance.now() - started) / 1000;
  if (run.status !== 0 && run.status !== 1) {
    throw new Error(`${which} lint ${target} exited ${run.status}: ${run.stderr}`);
  }
  return { stdout: run.stdout, document: JSON.parse(run.stdout), seconds };
}

/** Diagnostics keyed by file, as comparable strings with a fixed key order. */
function describe(document) {
  const files = new Map();
  for (const file of document.files) {
    const diagnostics = file.diagnostics.map((d) =>
      JSON.stringify([d.id, d.severity, d.range, d.message, d.fix ?? null]),
    );
    files.set(file.path, { encoding: file.encoding, diagnostics });
  }
  return files;
}

function compare(target, ts, rust) {
  const mismatches = [];
  const expected = describe(ts.document);
  const actual = describe(rust.document);

  for (const path of new Set([...expected.keys(), ...actual.keys()])) {
    const a = expected.get(path);
    const b = actual.get(path);
    if (!a || !b) {
      mismatches.push(`${path}: only in ${a ? 'ts' : 'rust'}`);
      continue;
    }
    if (a.encoding !== b.encoding) mismatches.push(`${path}: encoding ${a.encoding} vs ${b.encoding}`);
    const only = (from, other) => from.filter((d) => !other.includes(d));
    for (const d of only(a.diagnostics, b.diagnostics)) mismatches.push(`${path}: ts only ${d}`);
    for (const d of only(b.diagnostics, a.diagnostics)) mismatches.push(`${path}: rust only ${d}`);
    if (!mismatches.length && a.diagnostics.join() !== b.diagnostics.join()) {
      mismatches.push(`${path}: same diagnostics in a different order`);
    }
  }
  if (JSON.stringify(ts.document.summary) !== JSON.stringify(rust.document.summary)) {
    mismatches.push(`summary: ${JSON.stringify(ts.document.summary)} vs ${JSON.stringify(rust.document.summary)}`);
  }
  return mismatches;
}

let failed = false;
const report = [];
for (const target of targets) {
  const ts = lint('ts', target);
  const rust = lint('rust', target);
  const mismatches = compare(target, ts, rust);
  const diagnostics = ts.document.files.reduce((n, f) => n + f.diagnostics.length, 0);
  console.log(
    `${target}: ${ts.document.files.length} files, ${diagnostics} diagnostics, ${mismatches.length} mismatches, ` +
      `byte-identical output: ${ts.stdout === rust.stdout}, ts ${ts.seconds.toFixed(2)}s, rust ${rust.seconds.toFixed(2)}s`,
  );
  for (const line of mismatches.slice(0, 20)) console.log(`  ${line}`);
  if (mismatches.length > 20) console.log(`  ... ${mismatches.length - 20} more`);
  report.push(...mismatches.map((m) => `${target}: ${m}`));
  failed ||= mismatches.length > 0;
}
if (process.env.PARITY_REPORT) writeFileSync(process.env.PARITY_REPORT, report.join('\n'));
process.exit(failed ? 1 : 0);
