// A minimal LSP client over stdio, shared by lsp-parity.mjs and lsp-latency.mjs.
import { spawn } from 'node:child_process';

export class LspClient {
  /** `command` is the program, `args` its arguments; `wrap` runs it under another program. */
  constructor(command, args, { wrap = [] } = {}) {
    const [program, ...prefix] = [...wrap, command];
    this.child = spawn(program, [...prefix, ...args], { stdio: ['pipe', 'pipe', 'pipe'] });
    this.buffer = Buffer.alloc(0);
    this.nextId = 0;
    this.pending = new Map();
    this.waiters = [];
    this.notifications = [];
    this.stderr = '';
    this.child.stdout.on('data', (chunk) => this.onData(chunk));
    this.child.stderr.on('data', (chunk) => (this.stderr += chunk));
    this.closed = new Promise((resolve) => this.child.on('close', resolve));
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
      this.onMessage(JSON.parse(body));
    }
  }

  onMessage(message) {
    if (message.method && message.id !== undefined) {
      // The only request a server makes of us is the settings one.
      const result = message.method === 'workspace/configuration' ? [{ rules: {} }] : null;
      this.send({ id: message.id, result });
      return;
    }
    if (message.method) {
      this.notifications.push(message);
      this.waiters = this.waiters.filter((waiter) => !waiter(message));
      return;
    }
    const settle = this.pending.get(message.id);
    this.pending.delete(message.id);
    settle?.(message);
  }

  send(message) {
    const body = JSON.stringify({ jsonrpc: '2.0', ...message });
    this.child.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  }

  notify(method, params) {
    this.send({ method, params });
  }

  /** Resolves with the whole response message: `result` or `error`. */
  call(method, params) {
    const id = ++this.nextId;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      this.send({ id, method, params });
    });
  }

  /** Resolves with the next notification `match` accepts, from now on. */
  next(match) {
    return new Promise((resolve) => this.waiters.push((m) => match(m) && (resolve(m), true)));
  }

  async initialize(folders, configuration = true) {
    const params = {
      processId: null,
      rootUri: folders[0] ?? null,
      workspaceFolders: folders.map((uri) => ({ uri, name: uri })),
      capabilities: { workspace: { configuration } },
    };
    const reply = await this.call('initialize', params);
    this.notify('initialized', {});
    return reply.result;
  }

  async shutdown() {
    await this.call('shutdown', null);
    this.notify('exit', null);
    await this.closed;
  }
}
