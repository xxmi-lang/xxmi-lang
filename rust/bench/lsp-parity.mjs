#!/usr/bin/env node
// Parity check between the TypeScript language server and the Rust one. Each server is started over
// stdio on the same sessions: fixtures/ as a workspace with every fixture .ini open, and a ZZMI
// package as a workspace with its .ini files open. Per file it compares documentSymbol, then, at a
// deterministic capped sample of positions, hover, definition, references, completion,
// prepareRename and rename, plus codeAction for every diagnostic with a fix, plus the
// publishDiagnostics of each document after open, after an edit and after a close.
// Responses are compared as JSON with sorted keys. Exits 1 on any mismatch.
//
//   cargo build --release --manifest-path rust/Cargo.toml && node rust/bench/lsp-parity.mjs
//
// XXMI_RS overrides the Rust binary, XXMI_PACKAGE_SRC the ZZMI folder, LSP_PARITY_REPORT writes the
// mismatch list to a file (default: the OS temp dir).

import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { LspClient } from './lsp-client.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const rustBinary = process.env.XXMI_RS ?? join(root, 'rust/target/release/xxmi');
const zzmi = resolve(root, process.env.XXMI_PACKAGE_SRC ?? '../xxmi-refs/ZZMI-Package/ZZMI');
const reportPath = process.env.LSP_PARITY_REPORT ?? join(tmpdir(), 'xxmi-lsp-parity.json');

/** Positions per file, and requests in flight at once. */
const MAX_POSITIONS = 120;
const BATCH = 64;

const servers = {
  ts: () =>
    new LspClient('node', ['--conditions=source', join(root, 'packages/lsp/src/main.ts'), '--stdio']),
  rust: () => new LspClient(rustBinary, ['lsp', '--stdio']),
};

const sessions = [
  { name: 'fixtures', folder: join(root, 'fixtures') },
  { name: 'zzmi', folder: zzmi },
].filter((session) => {
  const found = existsSync(session.folder);
  if (!found) console.error(`skipping ${session.name}: ${session.folder} not found`);
  return found;
});

function iniFiles(dir) {
  const found = [];
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...iniFiles(path));
    else if (/\.ini$/i.test(entry.name) && statSync(path).size < 2_000_000) found.push(path);
  }
  return found;
}

/** Key-sorted JSON, so object key order can't matter. */
function canonical(value) {
  return JSON.stringify(value, (_key, v) =>
    v && typeof v === 'object' && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : 1)))
      : v,
  );
}

/** Interesting columns of a line: its start, first word, after `=`, middle and end. */
function columns(line) {
  const cols = new Set([0, line.length, Math.floor(line.length / 2)]);
  const word = /\w/.exec(line);
  if (word) cols.add(word.index + 1);
  const eq = line.indexOf('=');
  if (eq >= 0) [eq, eq + 1, eq + 2, eq + 5].forEach((c) => cols.add(Math.min(c, line.length)));
  const dollar = line.indexOf('$');
  if (dollar >= 0) cols.add(dollar + 2);
  return [...cols].sort((a, b) => a - b);
}

function samplePositions(text) {
  const lines = text.split('\n');
  const stride = Math.max(1, Math.ceil((lines.length * 4) / MAX_POSITIONS));
  const positions = [];
  for (let line = 0; line < lines.length && positions.length < MAX_POSITIONS; line += stride) {
    const content = lines[line].replace(/\r$/, '');
    for (const character of columns(content)) positions.push({ line, character });
  }
  return positions.slice(0, MAX_POSITIONS);
}

async function inBatches(items, run) {
  const out = [];
  for (let i = 0; i < items.length; i += BATCH) {
    out.push(...(await Promise.all(items.slice(i, i + BATCH).map(run))));
  }
  return out;
}

const answer = (message) => (message.error ? { error: message.error } : message.result);

/** Everything one server says about one session, keyed so the two servers can be diffed. */
async function collect(which, session) {
  const client = servers[which]();
  const files = iniFiles(session.folder);
  const record = new Map();
  const folder = pathToFileURL(session.folder).href;
  await client.initialize([folder]);

  const lastDiagnostics = new Map();
  const note = () => {
    for (const m of client.notifications.splice(0)) {
      if (m.method === 'textDocument/publishDiagnostics') lastDiagnostics.set(m.params.uri, m.params.diagnostics);
    }
  };

  const texts = new Map();
  for (const path of files) {
    const uri = pathToFileURL(path).href;
    const text = readFileSync(path, 'utf8');
    texts.set(uri, text);
    client.notify('textDocument/didOpen', {
      textDocument: { uri, languageId: 'migoto', version: 1, text },
    });
    // The response comes after the update that publishes this document's diagnostics.
    const symbols = await client.call('textDocument/documentSymbol', { textDocument: { uri } });
    note();
    record.set(`${path} documentSymbol`, answer(symbols));
    record.set(`${path} diagnostics@open`, lastDiagnostics.get(uri));
  }

  for (const [uri, text] of texts) {
    const path = new URL(uri).pathname;
    const requests = samplePositions(text).flatMap((position) => {
      const base = { textDocument: { uri }, position };
      return [
        ['hover', base],
        ['definition', base],
        ['references', { ...base, context: { includeDeclaration: true } }],
        ['references-', { ...base, context: { includeDeclaration: false } }],
        ['completion', base],
        ['prepareRename', base],
        ['rename', { ...base, newName: 'ParityRename' }],
      ];
    });
    const replies = await inBatches(requests, ([method, params]) =>
      client.call(`textDocument/${method.replace(/-$/, '')}`, params),
    );
    requests.forEach(([method, params], i) => {
      record.set(`${decodeURIComponent(path)} ${method} ${params.position.line}:${params.position.character}`, answer(replies[i]));
    });
  }

  await quickFixes(client, texts, lastDiagnostics, record, note);
  await edits(client, texts, record, lastDiagnostics, note);

  note();
  for (const [uri, diagnostics] of lastDiagnostics) record.set(`${uri} diagnostics@end`, diagnostics);
  await client.shutdown();
  return { record, stderr: client.stderr };
}

async function quickFixes(client, texts, lastDiagnostics, record, note) {
  note();
  for (const [uri, diagnostics] of [...lastDiagnostics]) {
    const fixable = diagnostics.filter((d) => d.data?.edits);
    if (!fixable.length) continue;
    const reply = await client.call('textDocument/codeAction', {
      textDocument: { uri },
      range: fixable[0].range,
      context: { diagnostics: fixable },
    });
    record.set(`${uri} codeAction`, answer(reply));
  }
}

/** Typing a wrong reference into the first documents, then closing one. */
async function edits(client, texts, record, lastDiagnostics, note) {
  const uris = [...texts.keys()].slice(0, 6);
  for (const [index, uri] of uris.entries()) {
    const at = { line: 0, character: 0 };
    client.notify('textDocument/didChange', {
      textDocument: { uri, version: 2 },
      contentChanges: [{ range: { start: at, end: at }, text: '[CommandListParityEdit]\nrun = CommandListMissing\n' }],
    });
    const symbols = await client.call('textDocument/documentSymbol', { textDocument: { uri } });
    note();
    record.set(`${uri} documentSymbol@edit`, answer(symbols));
    record.set(`${uri} diagnostics@edit`, lastDiagnostics.get(uri));
    if (index === 0) {
      client.notify('textDocument/didClose', { textDocument: { uri } });
      await client.call('textDocument/documentSymbol', { textDocument: { uri } });
      note();
      record.set(`${uri} diagnostics@close`, lastDiagnostics.get(uri));
    }
  }
}

function diff(session, ts, rust) {
  const mismatches = [];
  for (const key of new Set([...ts.record.keys(), ...rust.record.keys()])) {
    const expected = canonical(ts.record.get(key));
    const actual = canonical(rust.record.get(key));
    if (expected !== actual) mismatches.push({ session, key, ts: expected, rust: actual });
  }
  return mismatches;
}

const all = [];
let compared = 0;
for (const session of sessions) {
  const started = performance.now();
  const ts = await collect('ts', session);
  const rust = await collect('rust', session);
  const mismatches = diff(session.name, ts, rust);
  compared += ts.record.size;
  all.push(...mismatches);
  console.log(
    `${session.name}: ${ts.record.size} responses compared, ${mismatches.length} mismatches (${((performance.now() - started) / 1000).toFixed(1)} s)`,
  );
  if (rust.stderr) console.log(`rust stderr: ${rust.stderr.slice(0, 500)}`);
}

writeFileSync(reportPath, JSON.stringify(all, null, 2));
console.log(`${compared} responses, ${all.length} mismatches; report: ${reportPath}`);
for (const m of all.slice(0, 5)) console.log(`  ${m.session} ${m.key}\n    ts:   ${m.ts?.slice(0, 300)}\n    rust: ${m.rust?.slice(0, 300)}`);
process.exit(all.length === 0 ? 0 : 1);
