#!/usr/bin/env node
// Parity check between the TypeScript CLI and the Rust binary: runs `xxmi lint --format json` with
// both on fixtures/ and on a ZZMI package, and compares every diagnostic (id, severity, message,
// range, fix). Then compares `xxmi query` (text and --json: stdout, stderr and exit code) over every
// spec category and library symbols, and `xxmi index` output semantically. Exits 1 on any mismatch.
//
//   cargo build --release --manifest-path rust/Cargo.toml && node rust/bench/parity.mjs
//
// XXMI_RS overrides the Rust binary, XXMI_PACKAGE_SRC the ZZMI folder, PARITY_REPORT writes the
// full mismatch list to a file.

import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
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

function cli(which, args) {
  const [command, prefix] = commands[which];
  const run = spawnSync(command, [...prefix, ...args], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 30 });
  return { status: run.status, stdout: run.stdout, stderr: run.stderr };
}

/** A deterministic spread of `count` items. */
function sample(items, count) {
  const step = Math.max(1, Math.floor(items.length / count));
  return items.filter((_, i) => i % step === 0).slice(0, count);
}

function querySet() {
  const spec = JSON.parse(readFileSync(join(root, 'packages/core/dist/spec.json'), 'utf8'));
  const snapshot = JSON.parse(readFileSync(join(root, 'packages/core/snapshots/zzmi.json'), 'utf8'));
  const commandNames = [
    ...spec.commands.map((c) => c.name),
    ...spec.functions.map((f) => f.name),
    ...spec.resourceMembers.map((m) => `->${m.name}`),
  ];
  const queries = [
    ['section-kinds'],
    ...spec.sections.map((s) => ['keys', s.name]),
    ['keys', '[Resource]'],
    ['keys', 'NoSuchSection'],
    ['keys'],
    ...[...new Set(commandNames)].map((name) => ['command', name]),
    ['command', 'commandlistFoo'],
    ['command', 'nothing-like-this'],
    ['command'],
    ...spec.operators.tokens.map((t) => ['operator', t.token]),
    ['operator', '???'],
    ['symbol', 'CommandList\\ZZMI\\SetTextures'],
    ['symbol', '$\\TTL\\alpha'],
    ['symbol', 'CommandList\\Nope\\Nothing'],
    ['symbol'],
    ['library'],
    ['library', 'ttl'],
    ['library', 'nolib'],
    ['refs', 'x'],
    ['bogus'],
    [],
  ];
  for (const s of sample(snapshot.sections, 25)) queries.push(['symbol', s.qualifiedName]);
  for (const v of sample(snapshot.variables, 25)) queries.push(['symbol', v.key]);

  const withPackage = zzmi && existsSync(zzmi) ? ['--package', zzmi] : [];
  const out = [];
  for (const q of queries) {
    out.push(q, [...q, '--json']);
    if (['symbol', 'library'].includes(q[0])) out.push([...q, ...withPackage], [...q, ...withPackage, '--json']);
  }
  return out;
}

function compareQueries() {
  const mismatches = [];
  const queries = querySet();
  for (const args of queries) {
    const ts = cli('ts', ['query', ...args]);
    const rust = cli('rust', ['query', ...args]);
    const label = `query ${args.join(' ')}`;
    if (ts.status !== rust.status) mismatches.push(`${label}: exit ${ts.status} vs ${rust.status}`);
    else if (ts.stdout !== rust.stdout) mismatches.push(`${label}: stdout differs`);
    else if (ts.stderr !== rust.stderr && ts.status !== 3 && ts.status !== 2) {
      mismatches.push(`${label}: stderr ${JSON.stringify(ts.stderr)} vs ${JSON.stringify(rust.stderr)}`);
    }
  }
  console.log(`query: ${queries.length} invocations, ${mismatches.length} mismatches`);
  return mismatches;
}

function compareIndex() {
  if (!existsSync(zzmi)) return [];
  const dir = mkdtempSync(join(tmpdir(), 'xxmi-index-'));
  const outputs = {};
  for (const which of ['ts', 'rust']) {
    const out = join(dir, `${which}.json`);
    const run = cli(which, ['index', zzmi, '--out', out, '--name', 'zzmi']);
    outputs[which] = { run, text: existsSync(out) ? readFileSync(out, 'utf8') : '' };
  }
  const { ts, rust } = outputs;
  const mismatches = [];
  if (ts.run.status !== rust.run.status) mismatches.push(`index: exit ${ts.run.status} vs ${rust.run.status}`);
  const message = (run) => run.stderr.replace(/→ .*/, '→ OUT');
  if (message(ts.run) !== message(rust.run)) mismatches.push(`index: stderr ${ts.run.stderr} vs ${rust.run.stderr}`);
  const same = JSON.stringify(JSON.parse(ts.text)) === JSON.stringify(JSON.parse(rust.text));
  if (!same) mismatches.push('index: snapshot JSON differs');
  console.log(`index ${zzmi}: ${mismatches.length} mismatches, byte-identical output: ${ts.text === rust.text}`);
  rmSync(dir, { recursive: true, force: true });
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
const other = [...compareQueries(), ...compareIndex()];
for (const line of other.slice(0, 20)) console.log(`  ${line}`);
report.push(...other);
failed ||= other.length > 0;
if (process.env.PARITY_REPORT) writeFileSync(process.env.PARITY_REPORT, report.join('\n'));
process.exit(failed ? 1 : 0);
