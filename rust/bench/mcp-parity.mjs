// TS and Rust MCP servers, same requests, diffed answers (docs/rust-port.md).
//
//   node rust/bench/mcp-parity.mjs [--skip-index]
//
// Phase 1 runs the scenarios of packages/mcp-server/test against a fixture volume. Phase 2 lets both
// servers index the local checkouts in ../xxmi-refs (git `insteadOf` rewrites GitHub URLs; XXMITools
// has no local clone, so a one-commit stand-in is made) and diffs the questions of
// scripts/mcp-acceptance.ts plus the version listing. The GitHub releases API is the one live call.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { makeVolume, post, root, rpc, startServer } from './mcp-support.mjs';

const REFS = process.env.XXMI_REFS ?? resolve(root, '../xxmi-refs');
const TS_PORT = 3911;
const RUST_PORT = 3912;
// Rate limiting is timing-dependent; the http tests of both servers cover it.
const OPEN = { RATE_LIMIT_PER_MIN: '1000000', RATE_LIMIT_BURST: '1000000' };
const skipIndex = process.argv.includes('--skip-index');

const REMOTE_DIRS = {
  'SpectrumQT/XXMI-Libs-Package': 'XXMI-Libs-Package',
  'leotorrez/ZZMI-Package': 'ZZMI-Package',
  'bo3b/3Dmigoto': '3Dmigoto',
};

/** Keys that differ by run: wall-clock stamps and timings. */
const VOLATILE = new Set(['indexedAt', 'builtAt', 'ms']);

function normalise(value) {
  if (Array.isArray(value)) return value.map(normalise);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .filter(([key]) => !VOLATILE.has(key))
        .map(([key, inner]) => [key, normalise(inner)]),
    );
  }
  return value;
}

/** Tool results carry JSON as text when asked for it; compare that structurally too. */
function normaliseReply(reply) {
  const body = normalise(reply.body);
  const content = body?.result?.content;
  if (Array.isArray(content)) {
    for (const part of content) {
      if (typeof part.text === 'string') part.text = part.text.replace(/indexed [0-9T:.Z-]+/g, 'indexed <time>');
      if (typeof part.text !== 'string' || !part.text.startsWith('{')) continue;
      try {
        part.text = JSON.stringify(normalise(JSON.parse(part.text)), null, 1);
      } catch {
        // plain text
      }
    }
  }
  const kept = ['content-type', 'allow', 'retry-after'];
  const headers = Object.fromEntries(kept.filter((k) => reply.headers[k]).map((k) => [k, reply.headers[k]]));
  return { status: reply.status, headers, body };
}

const tool = (name, args) => rpc('tools/call', { name, arguments: args });

const SOURCE = { repo: 'dll', path: 'DirectX11/CommandList.cpp' };
const LINT_BAD = { 'Mods/A/mod.ini': '[TextureOverrideA]\nhash = 11111111\nrun = CommandListLocl\n' };
const LINT_LIB = {
  'Mods/A/mod.ini':
    '[Include Other]\ninclude = other.ini\n\n[TextureOverrideA]\nhash = 11111111\nrun = CommandList\\Demo\\SetThings\nrun = CommandList\\Shared\\Thing\n',
  'Mods/A/other.ini': 'namespace = Shared\n\n[CommandListThing]\n',
};

/** Tool calls both servers must answer alike. `volume` ones need the fixture volume. */
const TOOL_CASES = [
  ['spec_lookup', { name: 'store' }],
  ['spec_lookup', { name: 'store', format: 'json' }],
  ['spec_lookup', { name: 'ResourceFoo' }],
  ['spec_lookup', { name: 'hash', section: 'TextureOverride' }],
  ['spec_lookup', { name: 'match_dhash', dll_ref: 'head' }],
  ['spec_lookup', { name: 'nonsense' }],
  ['spec_lookup', { name: 'store', dll_ref: 'bundled' }],
  ['spec_lookup', { name: 'store', dll_ref: 'deadbeef1' }],
  ['spec_lookup', { name: 'store', dll_ref: 'v9.9.9' }],
  ['spec_lookup', { name: 'store', dll_ref: 'latest-prerelease' }],
  ['spec_lookup', { name: '' }],
  ['spec_lookup', {}],
  ['spec_list', { kind: 'command' }],
  ['spec_list', { kind: 'key', section: 'Resource' }],
  ['spec_list', { kind: 'key' }],
  ['spec_list', { kind: 'enum', format: 'json' }],
  ['spec_list', { kind: 'section' }],
  ['spec_list', { kind: 'command', cursor: '!!' }],
  ['spec_list', { kind: 'bogus' }],
  ['library_list', { game: 'zzmi' }],
  ['library_list', { game: 'zzmi', format: 'json' }],
  ['library_list', { game: 'zzmi', ref: 'v1.0.0' }],
  ['library_list', { game: 'zzmi', ref: 'v9' }],
  ['library_list', { game: 'nope' }],
  ['library_symbol', { qualified_name: 'CommandList\\Demo\\SetThings', game: 'zzmi' }],
  ['library_symbol', { qualified_name: 'CommandList\\ZZMI\\SetTextures', game: 'zzmi' }],
  ['library_symbol', { qualified_name: 'CommandList\\ZZMI\\SetTextures', game: 'zzmi', format: 'json' }],
  ['library_symbol', { qualified_name: '$\\Demo\\skip_glow', game: 'zzmi' }],
  ['library_symbol', { qualified_name: 'CommandList\\Demo\\SetThing', game: 'zzmi' }],
  ['find_implementation', { feature: 'store' }],
  ['find_implementation', { feature: 'store', format: 'json' }],
  ['find_implementation', { feature: 'store', ref: 'v1.2.3' }],
  ['find_implementation', { feature: 'nonexistent_feature_zzz' }],
  ['read_source', { ...SOURCE, start_line: 3, end_line: 5 }],
  ['read_source', { ...SOURCE, start_line: 1, end_line: 500 }],
  ['read_source', { ...SOURCE, start_line: 5, end_line: 3 }],
  ['read_source', { ...SOURCE, start_line: 9999, end_line: 10000 }],
  ['read_source', { ...SOURCE, start_line: 1, end_line: 2, format: 'json' }],
  ...['../secret', '/etc/passwd', 'C:\\Windows\\win.ini', 'a/../../x', 'nope.cpp'].map((path) => [
    'read_source',
    { repo: 'dll', path, start_line: 1, end_line: 2 },
  ]),
  ['read_source', { repo: 'someone/else', path: 'a', start_line: 1, end_line: 2 }],
  ['search_source', { repo: 'dll', query: 'store' }],
  ['search_source', { repo: 'dll', query: 'store', max_results: 2, format: 'json' }],
  ['search_source', { repo: 'dll', query: 'store', path_glob: '**/*.cpp' }],
  ['search_source', { repo: 'dll', query: '(' }],
  ['search_source', { repo: 'someone/else', query: 'x' }],
  ['list_versions', {}],
  ['list_versions', { format: 'json' }],
  ['list_versions', { repo: 'dll' }],
  ['list_versions', { repo: 'nope' }],
  ['lint_ini', { files: LINT_BAD }],
  ['lint_ini', { files: LINT_BAD, format: 'json' }],
  ['lint_ini', { files: LINT_BAD, dll_ref: 'head' }],
  ['lint_ini', { files: LINT_LIB }],
  ['lint_ini', { files: LINT_LIB, game: 'zzmi' }],
  ['lint_ini', { files: { 'a.ini': '[Include Evil]\ninclude = ..\\..\\..\\etc\\hosts\n' }, format: 'json' }],
  ...['../x.ini', '/abs.ini', 'C:\\x.ini', 'a/../b.ini', 'x.txt', ''].map((path) => ['lint_ini', { files: { [path]: '' } }]),
  ['lint_ini', { files: { 'A.ini': '', 'a.INI': '' } }],
  ['lint_ini', { files: {} }],
  ['lint_ini', {}],
  ['nope_tool', {}],
];

const RAW_CASES = [
  ['GET /healthz', (p) => fetchRaw(p, '/healthz')],
  ['GET /mcp', (p) => fetchRaw(p, '/mcp')],
  ['DELETE /mcp', (p) => fetchRaw(p, '/mcp', { method: 'DELETE' })],
  ['GET /nope', (p) => fetchRaw(p, '/nope')],
  ['POST /nope', (p) => fetchRaw(p, '/nope', { method: 'POST', body: '{}' })],
  ['invalid json', (p) => post(p, '{nope')],
  ['empty body', (p) => post(p, '')],
  ['not an object', (p) => post(p, '[1]')],
  ['unknown method', (p) => post(p, rpc('nope/method', {}))],
  ['initialize', (p) => post(p, rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'x', version: '1' } }))],
  ['initialize old protocol', (p) => post(p, rpc('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'x', version: '1' } }))],
  ['ping', (p) => post(p, rpc('ping'))],
  ['tools/list', (p) => post(p, rpc('tools/list'))],
  ['notification', (p) => post(p, { jsonrpc: '2.0', method: 'notifications/initialized' })],
  ['batch', (p) => post(p, [rpc('ping', undefined, 1), rpc('tools/list', undefined, 2)])],
  ['no accept header', (p) => post(p, rpc('ping'), { 'content-type': 'application/json' })],
  ['wrong content type', (p) => post(p, rpc('ping'), { 'content-type': 'text/plain', accept: 'application/json, text/event-stream' })],
  ['admin refresh hidden', (p) => fetchRaw(p, '/admin/refresh', { method: 'POST' })],
    ['tools/call bad params', (p) => post(p, rpc('tools/call', {}))],
  ['resources/list', (p) => post(p, rpc('resources/list'))],
  ['prompts/list', (p) => post(p, rpc('prompts/list'))],
];

async function fetchRaw(port, path, init = {}) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, init);
  const text = await res.text();
  let body = text;
  try {
    body = JSON.parse(text);
  } catch {
    // keep text
  }
  return { status: res.status, headers: Object.fromEntries(res.headers), body };
}

let mismatches = 0;
let compared = 0;

async function compare(label, ask) {
  const safe = (port) => ask(port).catch((e) => ({ status: 0, headers: {}, body: String(e.cause?.code ?? e.message) }));
  const [ts, rust] = await Promise.all([safe(TS_PORT), safe(RUST_PORT)]);
  const [a, b] = [JSON.stringify(normaliseReply(ts), null, 1), JSON.stringify(normaliseReply(rust), null, 1)];
  compared += 1;
  if (a === b) return;
  mismatches += 1;
  console.log(`MISMATCH  ${label}`);
  const al = a.split('\n');
  const bl = b.split('\n');
  let shown = 0;
  for (let i = 0; i < Math.max(al.length, bl.length) && shown < 6; i++) {
    if (al[i] === bl[i]) continue;
    console.log(`   ts:   ${(al[i] ?? '<none>').slice(0, 300)}\n   rust: ${(bl[i] ?? '<none>').slice(0, 300)}`);
    shown += 1;
  }
}

// A server may answer 413 or drop the connection while the client still writes; both refuse the body.
const oversized = (p) =>
  post(p, 'x'.repeat(3_500_000)).catch(() => ({ status: 413, headers: {}, body: 'refused' })).then((r) => (r.status === 413 ? { status: 413, headers: {}, body: 'refused' } : r));

async function runCases() {
  await compare('oversized body', oversized);
  for (const [name, args] of TOOL_CASES) {
    await compare(`${name} ${JSON.stringify(args).slice(0, 100)}`, (p) => post(p, tool(name, args)));
  }
  for (const [label, ask] of RAW_CASES) await compare(label, ask);
}

function gitConfigFor(dir) {
  const rewrites = Object.entries(REMOTE_DIRS)
    .map(([full, local]) => `[url "${join(REFS, local)}"]\n\tinsteadOf = https://github.com/${full}.git\n`)
    .concat(`[url "${join(dir, 'XXMITools')}"]\n\tinsteadOf = https://github.com/leotorrez/XXMITools.git\n`);
  const file = join(dir, 'gitconfig');
  writeFileSync(file, `[safe]\n\tdirectory = *\n${rewrites.join('')}`);
  return file;
}

function makeToolsRemote(dir) {
  const repo = join(dir, 'XXMITools');
  mkdirSync(repo, { recursive: true });
  const git = (...args) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...args], { cwd: repo });
  git('init', '-q', '-b', 'main');
  writeFileSync(join(repo, 'README.md'), 'XXMITools stand-in\n');
  git('add', '-A');
  git('commit', '-q', '-m', 'init');
}

async function waitIndexed(port, label) {
  const deadline = Date.now() + 600_000;
  for (;;) {
    const reply = await post(port, tool('list_versions', { repo: 'dll', format: 'json' })).catch(() => undefined);
    const text = reply?.body?.result?.content?.[0]?.text ?? '';
    const ready = (await fetchRaw(port, '/healthz').catch(() => undefined))?.body?.ready;
    if (ready && text && !text.includes('"pending"')) return;
    if (Date.now() > deadline) throw new Error(`${label} did not finish indexing`);
    await new Promise((r) => setTimeout(r, 1000));
  }
}

const ACCEPTANCE = [
  ['library_symbol', { qualified_name: 'CommandList\\ZZMI\\SetTextures', game: 'zzmi' }],
  ['find_implementation', { feature: 'store' }],
  ['lint_ini', { files: { 'Mods/Demo/mod.ini': '[TextureOverrideDemo]\nhash = 12345678\nrun = CommandListLocl\n' }, game: 'zzmi' }],
  ['lint_ini', { files: { 'Mods/Demo/mod.ini': '[TextureOverrideDemo]\nhash = 12345678\nrun = CommandList\\ZZMI\\SetTextures\n' }, game: 'zzmi' }],
  ['spec_lookup', { name: 'match_dhash', dll_ref: 'head' }],
  ['spec_lookup', { name: 'match_dhash' }],
  ['spec_lookup', { name: 'snapshotinitialtextures', dll_ref: 'v1.3.2' }],
  ['spec_lookup', { name: 'snapshotinitialtextures', dll_ref: 'v1.3.1' }],
  ['spec_list', { kind: 'command', dll_ref: 'v1.2.0' }],
  ['library_list', { game: 'zzmi' }],
  ['list_versions', {}],
  ['list_versions', { format: 'json' }],
  ['search_source', { repo: 'zzmi', query: 'SetTextures', max_results: 5, path_glob: '**/TTLib.ini' }],
  // rg walks files in parallel, so unrestricted searches list files in a run-dependent order.,
  ['read_source', { repo: 'dll', path: 'DirectX11/CommandList.cpp', start_line: 1, end_line: 30 }],
];

async function phaseOne(scratch) {
  const volumes = [join(scratch, 'ts-fixture'), join(scratch, 'rust-fixture')];
  for (const dir of volumes) makeVolume(dir);
  const servers = [await startServer('ts', TS_PORT, volumes[0], OPEN), await startServer('rust', RUST_PORT, volumes[1], OPEN)];
  try {
    console.log('phase 1: fixture volume');
    await runCases();
  } finally {
    for (const server of servers) server.kill();
  }
}

/** The M6 questions of docs/06, run by their own script against the Rust server. */
function acceptance(port) {
  try {
    execFileSync('node', ['scripts/mcp-acceptance.ts', `http://127.0.0.1:${port}/mcp`], { cwd: root, stdio: 'inherit' });
  } catch {
    mismatches += 1;
    console.log('MISMATCH  scripts/mcp-acceptance.ts failed against the Rust server');
  }
}

async function phaseTwo(scratch) {
  makeToolsRemote(scratch);
  const env = { ...OPEN, INDEXER: 'on', POLL_INTERVAL_MIN: '600', GIT_CONFIG_GLOBAL: gitConfigFor(scratch) };
  const servers = [
    await startServer('ts', TS_PORT, join(scratch, 'ts-data'), env),
    await startServer('rust', RUST_PORT, join(scratch, 'rust-data'), env),
  ];
  try {
    console.log('phase 2: both servers index the local refs');
    await Promise.all([waitIndexed(TS_PORT, 'ts'), waitIndexed(RUST_PORT, 'rust')]);
    for (const [name, args] of ACCEPTANCE) {
      await compare(`indexed ${name} ${JSON.stringify(args).slice(0, 100)}`, (p) => post(p, tool(name, args)));
    }
    await compare('indexed tools/list', (p) => post(p, rpc('tools/list')));
    acceptance(RUST_PORT);
  } finally {
    for (const server of servers) server.kill();
  }
}

const scratch = mkdtempSync(join(tmpdir(), 'xxmi-mcp-parity-'));
try {
  await phaseOne(scratch);
  if (!skipIndex) await phaseTwo(scratch);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log(`\n${compared} compared, ${mismatches} mismatched`);
process.exit(mismatches === 0 ? 0 : 1);
