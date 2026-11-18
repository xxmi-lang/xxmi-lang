#!/usr/bin/env node
// Smoke test of a language server over stdio: initialize, then shutdown and exit. Exits 1 if the
// reply has no capabilities or the server exits non-zero. release.yml runs it on each target.
//
//   node rust/bench/lsp-smoke.mjs dist/bin/xxmi-lsp --stdio
//   node rust/bench/lsp-smoke.mjs dist/bin/xxmi lsp

import { resolve } from 'node:path';
import { LspClient } from './lsp-client.mjs';

const TIMEOUT_MS = 30_000;

const [program, ...args] = process.argv.slice(2);
if (!program) {
  console.error('usage: node rust/bench/lsp-smoke.mjs <program> [arguments]');
  process.exit(2);
}

setTimeout(() => {
  console.error(`no answer from ${program} within ${TIMEOUT_MS} ms`);
  process.exit(1);
}, TIMEOUT_MS).unref();

const client = new LspClient(resolve(program), args);
// Without the configuration capability the server asks nothing, so shutdown cannot race a reply to it.
const reply = await client.initialize([], false);
if (!reply?.capabilities) {
  console.error(`initialize reply without capabilities: ${JSON.stringify(reply)}`);
  client.child.kill();
  process.exit(1);
}

await client.shutdown();
if (client.child.exitCode !== 0) {
  console.error(`${program} exited with ${client.child.exitCode}\n${client.stderr}`);
  process.exit(1);
}
console.log(`initialize and shutdown ok: ${program} ${args.join(' ')}`);
