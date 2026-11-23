// Shared by mcp-parity.mjs and mcp-load.mjs: a fixture data volume and the two servers on it.

import { execFileSync, spawn } from 'node:child_process';
import { cpSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixtures = join(root, 'packages/mcp-server/test/fixtures');

const SHA_A = 'a'.repeat(40);
const SHA_TAG = 'c'.repeat(40);

const REPOS = [
  { alias: 'dll', owner: 'SpectrumQT', name: 'XXMI-Libs-Package', fixture: 'dll', branch: 'master' },
  { alias: 'zzmi', owner: 'leotorrez', name: 'ZZMI-Package', fixture: 'zzmi-package', branch: 'main', tag: 'v1.0.0' },
  { alias: 'xxmitools', owner: 'leotorrez', name: 'XXMITools', fixture: 'xxmitools', branch: 'main' },
  { alias: 'migoto', owner: 'bo3b', name: '3Dmigoto', fixture: 'dll', branch: 'master' },
];

/** A volume as the indexer leaves it: checkouts of the test fixtures and the state file. */
export function makeVolume(dir) {
  const state = { version: 1, repos: {}, releases: {} };
  for (const repo of REPOS) {
    const shas = repo.tag ? [SHA_A, SHA_TAG] : [SHA_A];
    for (const sha of shas) {
      const target = join(dir, 'repos', repo.owner, repo.name, sha);
      mkdirSync(target, { recursive: true });
      cpSync(join(fixtures, repo.fixture), target, { recursive: true });
    }
    state.repos[repo.alias] = {
      branch: { name: repo.branch, sha: SHA_A, kind: 'head' },
      tags: repo.tag ? [{ name: repo.tag, sha: SHA_TAG, kind: 'tag' }] : [],
      indexedAt: '2026-01-01T00:00:00.000Z',
    };
  }
  writeFileSync(join(dir, 'state.json'), JSON.stringify(state));
}

const REFS = process.env.XXMI_REFS ?? resolve(root, '../xxmi-refs');
const REMOTE_DIRS = {
  'SpectrumQT/XXMI-Libs-Package': 'XXMI-Libs-Package',
  'leotorrez/ZZMI-Package': 'ZZMI-Package',
  'bo3b/3Dmigoto': '3Dmigoto',
};

/**
 * Env that makes git read the tracked GitHub repos from local clones in `../xxmi-refs`, so a server
 * can index without a network. XXMITools has no clone there; a one-commit stand-in is made in `dir`.
 */
export function localRemotesEnv(dir) {
  const repo = join(dir, 'XXMITools');
  mkdirSync(repo, { recursive: true });
  const git = (...args) => execFileSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@t', ...args], { cwd: repo });
  git('init', '-q', '-b', 'main');
  writeFileSync(join(repo, 'README.md'), 'XXMITools stand-in\n');
  git('add', '-A');
  git('commit', '-q', '-m', 'init');

  const rewrites = Object.entries(REMOTE_DIRS)
    .map(([full, local]) => [full, join(REFS, local)])
    .concat([['leotorrez/XXMITools', repo]])
    .map(([full, path]) => `[url "${path}"]\n\tinsteadOf = https://github.com/${full}.git\n`);
  const file = join(dir, 'gitconfig');
  writeFileSync(file, `[safe]\n\tdirectory = *\n${rewrites.join('')}`);
  return { GIT_CONFIG_GLOBAL: file };
}

const COMMANDS = {
  ts: () => ['node', ['--conditions=source', join(root, 'packages/mcp-server/src/main.ts')]],
  rust: () => [process.env.XXMI_MCP_RS ?? join(root, 'rust/target/release/xxmi-mcp'), []],
};

/** Starts one server (`ts` or `rust`) on `port`; resolves once /healthz answers. */
export async function startServer(kind, port, dataDir, env = {}) {
  const [command, args] = COMMANDS[kind]();
  const child = spawn(command, args, {
    cwd: root,
    env: { ...process.env, PORT: String(port), HOST: '127.0.0.1', DATA_DIR: dataDir, INDEXER: 'off', ...env },
    stdio: ['ignore', 'ignore', 'inherit'],
  });
  const deadline = Date.now() + 30_000;
  for (;;) {
    try {
      if ((await fetch(`http://127.0.0.1:${port}/healthz`)).ok) return child;
    } catch {
      // not listening yet
    }
    if (Date.now() > deadline) throw new Error(`${kind} server did not start`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

const HEADERS = { 'content-type': 'application/json', accept: 'application/json, text/event-stream' };

/** One POST to /mcp; returns status and the parsed body (or the raw text). */
export async function post(port, body, headers = HEADERS, path = '/mcp') {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: 'POST',
    headers,
    body: typeof body === 'string' ? body : JSON.stringify(body),
  });
  const text = await res.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = text;
  }
  return { status: res.status, headers: Object.fromEntries(res.headers), body: json };
}

export const rpc = (method, params, id = 1) => ({ jsonrpc: '2.0', id, method, params });
