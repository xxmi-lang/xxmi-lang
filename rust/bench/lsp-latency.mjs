// Edit-to-diagnostics latency of `xxmi-rs lsp --stdio` on a ~5,000-line document.
//
//   node rust/bench/lsp-latency.mjs [path/to/xxmi-rs]      measure
//   node rust/bench/lsp-latency.mjs --write-doc FILE       write the document (for the in-process example)
//
// The document is the one scripts/perf-lsp.ts builds (modText(999, 280)).
import { spawn, execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const EDITS = 200;

export function modText(n, sections) {
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

if (process.argv[2] === '--write-doc') {
  writeFileSync(process.argv[3], modText(999, 280));
  process.exit(0);
}

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const p95 = (xs) => [...xs].sort((a, b) => a - b)[Math.ceil(xs.length * 0.95) - 1];

class Client {
  constructor(command) {
    // /usr/bin/time -l reports the server's peak RSS when it exits.
    this.child = spawn('/usr/bin/time', ['-l', command, 'lsp', '--stdio']);
    this.buffer = Buffer.alloc(0);
    this.waiters = [];
    this.timeOutput = '';
    this.child.stdout.on('data', (chunk) => this.onData(chunk));
    this.child.stderr.on('data', (chunk) => (this.timeOutput += chunk));
  }

  onData(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const end = this.buffer.indexOf('\r\n\r\n');
      if (end < 0) return;
      const length = Number(/Content-Length: (\d+)/i.exec(this.buffer.subarray(0, end).toString())[1]);
      if (this.buffer.length < end + 4 + length) return;
      const body = this.buffer.subarray(end + 4, end + 4 + length).toString();
      this.buffer = this.buffer.subarray(end + 4 + length);
      const message = JSON.parse(body);
      this.waiters = this.waiters.filter((waiter) => !waiter(message));
    }
  }

  send(message) {
    const body = JSON.stringify({ jsonrpc: '2.0', ...message });
    this.child.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  }

  next(match) {
    return new Promise((resolve) => this.waiters.push((m) => match(m) && (resolve(m), true)));
  }

  exited() {
    return new Promise((resolve) => this.child.on('close', resolve));
  }
}

const rssKb = (client) => {
  const pid = execFileSync('pgrep', ['-P', String(client.child.pid)]).toString().trim();
  return Number(execFileSync('ps', ['-o', 'rss=', '-p', pid]).toString());
};

const command = process.argv[2] ?? 'rust/target/release/xxmi-rs';
const client = new Client(command);
const uri = 'file:///bench/big.ini';
const lines = modText(999, 280).split('\n');
const published = (version) => (m) =>
  m.method === 'textDocument/publishDiagnostics' && m.params.version === version;

const initialized = client.next((m) => m.id === 1);
client.send({ id: 1, method: 'initialize', params: { processId: null, rootUri: null, capabilities: {} } });
await initialized;
client.send({ method: 'initialized', params: {} });

let start = performance.now();
const opened = client.next(published(1));
client.send({
  method: 'textDocument/didOpen',
  params: { textDocument: { uri, languageId: 'migoto', version: 1, text: lines.join('\n') } },
});
await opened;
const firstMs = performance.now() - start;
const idleKb = rssKb(client);

// Deterministic spread over the document; a changed line stays changed, like typing.
const samples = [];
let seed = 12345;
for (let i = 0; i < EDITS; i++) {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  const line = seed % lines.length;
  const character = Math.min(lines[line].length, seed % 7);
  lines[line] = lines[line].slice(0, character) + 'x' + lines[line].slice(character);

  const version = i + 2;
  const reply = client.next(published(version));
  start = performance.now();
  client.send({
    method: 'textDocument/didChange',
    params: {
      textDocument: { uri, version },
      contentChanges: [
        { range: { start: { line, character }, end: { line, character } }, text: 'x' },
      ],
    },
  });
  await reply;
  samples.push(performance.now() - start);
}

const shutdown = client.next((m) => m.id === 2);
client.send({ id: 2, method: 'shutdown' });
await shutdown;
client.send({ method: 'exit' });
await client.exited();

const peak = /(\d+)\s+maximum resident set size/.exec(client.timeOutput);
const fmt = (ms) => `${ms.toFixed(2)} ms`;
console.log(`lines: ${lines.length}`);
console.log(`first diagnostics after didOpen: ${fmt(firstMs)}`);
console.log(`didChange to publishDiagnostics over ${EDITS} edits: median ${fmt(median(samples))}, p95 ${fmt(p95(samples))}, max ${fmt(Math.max(...samples))}`);
console.log(`RSS idle after open: ${(idleKb / 1024).toFixed(1)} MB, peak: ${(Number(peak?.[1]) / 1024 / 1024).toFixed(1)} MB`);
