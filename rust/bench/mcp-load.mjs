// Load and memory of the TS and Rust MCP servers on the same fixture volume (docs/rust-port.md).
//
//   node rust/bench/mcp-load.mjs [seconds] [concurrency]
//
// A mix of lint_ini (about 200 lines), a spec_lookup and /healthz, then RSS idle, at peak under load
// and at peak while the indexer builds the DLL specs from the local refs in ../xxmi-refs.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { localRemotesEnv, makeVolume, post, rpc, startServer } from './mcp-support.mjs';

const SECONDS = Number(process.argv[2] ?? 10);
const CONCURRENCY = Number(process.argv[3] ?? 32);
const OPEN = { RATE_LIMIT_PER_MIN: '1000000', RATE_LIMIT_BURST: '1000000' };

const INI = Array.from({ length: 50 }, (_, i) =>
  `[TextureOverride${i}]\nhash = ${(10000000 + i).toString(16).padStart(8, '0')}\nrun = CommandListMissing${i}\nthis = ResourceX${i}\n`,
).join('\n');

let PORT = 0;

const REQUESTS = [
  () => post(PORT, rpc('tools/call', { name: 'lint_ini', arguments: { files: { 'Mods/A/mod.ini': INI } } })),
  () => post(PORT, rpc('tools/call', { name: 'spec_lookup', arguments: { name: 'store' } })),
  () => fetch(`http://127.0.0.1:${PORT}/healthz`).then((r) => r.text()),
];

/** Resident memory in MB; `ps` reports KB on macOS and Linux. */
function rssMb(pid) {
  return Number(execFileSync('ps', ['-o', 'rss=', '-p', String(pid)]).toString().trim()) / 1024;
}

function percentile(sorted, p) {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))];
}

async function load() {
  const latencies = [];
  const end = Date.now() + SECONDS * 1000;
  let next = 0;
  const worker = async () => {
    while (Date.now() < end) {
      const started = performance.now();
      await REQUESTS[next++ % REQUESTS.length]();
      latencies.push(performance.now() - started);
    }
  };
  await Promise.all(Array.from({ length: CONCURRENCY }, worker));
  latencies.sort((a, b) => a - b);
  return { rps: latencies.length / SECONDS, p50: percentile(latencies, 0.5), p99: percentile(latencies, 0.99) };
}

/** Samples the server's RSS until `stop()` is called; resolves the peak. */
function samplePeak(pid) {
  let peak = rssMb(pid);
  const timer = setInterval(() => {
    peak = Math.max(peak, rssMb(pid));
  }, 100);
  return () => {
    clearInterval(timer);
    return peak;
  };
}

async function measure(kind, port, scratch) {
  PORT = port;
  const server = await startServer(kind, port, join(scratch, `${kind}-fixture`), OPEN);
  try {
    await new Promise((r) => setTimeout(r, 1500));
    const idle = rssMb(server.pid);
    const stop = samplePeak(server.pid);
    const result = await load();
    return { ...result, idle, peak: stop() };
  } finally {
    server.kill();
  }
}

async function indexingPeak(kind, port, scratch) {
  const server = await startServer(kind, port, join(scratch, `${kind}-index`), {
    ...OPEN,
    INDEXER: 'on',
    ...localRemotesEnv(join(scratch, `${kind}-remotes`)),
  });
  const stop = samplePeak(server.pid);
  try {
    await waitIndexed(port);
    return stop();
  } finally {
    server.kill();
  }
}

async function waitIndexed(port) {
  for (;;) {
    const ready = await fetch(`http://127.0.0.1:${port}/healthz`).then((r) => r.json());
    const versions = await post(port, rpc('tools/call', { name: 'list_versions', arguments: { repo: 'dll', format: 'json' } }));
    const text = versions.body?.result?.content?.[0]?.text ?? '';
    if (ready.ready && text && !text.includes('"pending"')) return;
    await new Promise((r) => setTimeout(r, 500));
  }
}

const scratch = mkdtempSync(join(tmpdir(), 'xxmi-mcp-load-'));
try {
  for (const kind of ['ts', 'rust']) makeVolume(join(scratch, `${kind}-fixture`));
  const ports = { ts: 3921, rust: 3922 };
  for (const kind of ['ts', 'rust']) {
    const r = await measure(kind, ports[kind], scratch);
    const build = await indexingPeak(kind, ports[kind] + 10, scratch);
    console.log(
      `${kind.padEnd(4)} ${r.rps.toFixed(0)} req/s  p50 ${r.p50.toFixed(1)} ms  p99 ${r.p99.toFixed(1)} ms  ` +
        `RSS idle ${r.idle.toFixed(0)} MB  load peak ${r.peak.toFixed(0)} MB  indexing peak ${build.toFixed(0)} MB`,
    );
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
