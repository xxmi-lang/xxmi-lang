#!/usr/bin/env node
// Latency, memory and start-up of `xxmi lsp --stdio` against the docs/03-lsp.md targets, on a
// ~5,000-line document (the one scripts/perf-lsp.ts builds, modText(999, 280)) opened next to a ZZMI
// package workspace.
//
//   node rust/bench/lsp-latency.mjs            measure rust/target/release/xxmi
//   XXMI_RS=path node rust/bench/lsp-latency.mjs
//   XXMI_TRACE_TIMING=1 node rust/bench/lsp-latency.mjs    adds the per-span breakdown of an edit
//
// Edits are spaced past the 50 ms debounce so each one is linted on the leading edge; the server
// publishes without a document version, so a reply is the next publishDiagnostics for the document.
import { execFileSync } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { LspClient } from './lsp-client.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.XXMI_RS ?? join(root, 'rust/target/release/xxmi');
const zzmi = resolve(root, process.env.XXMI_PACKAGE_SRC ?? '../xxmi-refs/ZZMI-Package/ZZMI');
const EDITS = 100;
const SPACING_MS = 200;
const TARGETS = { cold: 1000, edit: 100, rssMb: 200 };

function modText(n, sections) {
  const out = [`namespace = PerfMod${n}`, ''];
  for (let i = 0; i < sections; i++) {
    out.push(
      `[TextureOverrideComponent${i}]`,
      `hash = ${(0x10000000 + i).toString(16)}`,
      'match_first_index = 0',
      'handling = skip',
      `ib = ResourceIndex${i}`,
      'if $active && DRAW_TYPE == 1',
      `\tps-t0 = ResourceDiffuse${i}`,
      '\trun = CommandListShared',
      'endif',
      'drawindexed = auto',
      '',
      `[ResourceIndex${i}]`,
      'type = Buffer',
      'format = DXGI_FORMAT_R32_UINT',
      '',
      `[ResourceDiffuse${i}]`,
      'filename = diffuse.dds',
      '',
    );
  }
  out.push('[Constants]', 'global $active = 1', '', '[CommandListShared]', 'run = CommandList\\ZZMI\\SetTextures', '');
  return out.join('\n');
}

const sorted = (xs) => [...xs].sort((a, b) => a - b);
const median = (xs) => sorted(xs)[Math.floor(xs.length / 2)];
const p95 = (xs) => sorted(xs)[Math.ceil(xs.length * 0.95) - 1];
const fmt = (ms) => `${ms.toFixed(1)} ms`;
const rssMb = (pid) => Number(execFileSync('ps', ['-o', 'rss=', '-p', String(pid)]).toString()) / 1024;

const forDocument = (uri) => (m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri;

async function edits(client, uri, lines) {
  const samples = [];
  let seed = 12345;
  for (let i = 0; i < EDITS; i++) {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    const line = seed % lines.length;
    const character = Math.min(lines[line].length, seed % 7);
    lines[line] = lines[line].slice(0, character) + 'x' + lines[line].slice(character);

    await new Promise((resolve) => setTimeout(resolve, SPACING_MS));
    const reply = client.next(forDocument(uri));
    const start = performance.now();
    client.notify('textDocument/didChange', {
      textDocument: { uri, version: i + 2 },
      contentChanges: [{ range: { start: { line, character }, end: { line, character } }, text: 'x' }],
    });
    await reply;
    samples.push(performance.now() - start);
  }
  return samples;
}

// With XXMI_TRACE_TIMING=1 the server prints one "xxmi-trace <label> <ms>" line per span; the
// median per label over the run is the breakdown of one edit.
function printBreakdown(stderr) {
  const byLabel = new Map();
  for (const [, label, ms] of stderr.matchAll(/^xxmi-trace (.+) ([\d.]+)$/gm)) {
    byLabel.set(label, [...(byLabel.get(label) ?? []), Number(ms)]);
  }
  for (const [label, times] of byLabel) {
    console.log(`trace ${label}: median ${median(times).toFixed(3)} ms over ${times.length}`);
  }
}

const hasPackage = existsSync(zzmi);
if (!hasPackage) console.error(`no ZZMI package at ${zzmi}: measuring without a workspace`);
const folders = hasPackage ? [pathToFileURL(zzmi).href] : [];
const uri = pathToFileURL(join(hasPackage ? zzmi : root, 'Mods/Perf/big.ini')).href;
const lines = modText(999, 280).split('\n');

const started = performance.now();
const client = new LspClient(binary, ['lsp', '--stdio']);
await client.initialize(folders);
const opened = client.next(forDocument(uri));
client.notify('textDocument/didOpen', {
  textDocument: { uri, languageId: 'migoto', version: 1, text: lines.join('\n') },
});
await opened;
const coldMs = performance.now() - started;
const idleMb = rssMb(client.child.pid);

const samples = await edits(client, uri, lines);
const peakMb = rssMb(client.child.pid);
await client.shutdown();

const results = [
  ['lines', lines.length, ''],
  ['cold start to first diagnostics (ZZMI + one mod)', fmt(coldMs), `< ${TARGETS.cold} ms: ${coldMs < TARGETS.cold ? 'met' : 'MISSED'}`],
  ['edit to diagnostics, median', fmt(median(samples)), ''],
  ['edit to diagnostics, p95', fmt(p95(samples)), ''],
  ['edit to diagnostics, max', fmt(Math.max(...samples)), `< ${TARGETS.edit} ms: ${Math.max(...samples) < TARGETS.edit ? 'met' : 'MISSED'}`],
  ['RSS after open', `${idleMb.toFixed(1)} MB`, ''],
  ['RSS after edits', `${peakMb.toFixed(1)} MB`, `< ${TARGETS.rssMb} MB: ${peakMb < TARGETS.rssMb ? 'met' : 'MISSED'}`],
  ['binary size', `${statSync(binary).size} B`, ''],
];
if (process.env.XXMI_TRACE_TIMING === '1') printBreakdown(client.stderr);
for (const [name, value, target] of results) console.log(`${name}: ${value}${target && ` (${target})`}`);
process.exit(results.some(([, , target]) => target.endsWith('MISSED')) ? 1 : 0);
