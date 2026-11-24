/**
 * A JSON-RPC client for the Rust language server, shared by the protocol suites.
 * Set XXMI_LSP_COMMAND to a JSON array, e.g. `["dist/bin/xxmi-lsp", "--stdio"]`.
 */
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import {
  createProtocolConnection,
  StreamMessageReader,
  StreamMessageWriter,
  type Diagnostic,
  type ProtocolConnection,
  type PublishDiagnosticsParams,
} from 'vscode-languageserver-protocol/node';

export const uri = (path: string): string => pathToFileURL(path).href;

const DEFAULT_TIMEOUT_MS = 15_000;

function serverCommand(): string[] {
  const json = process.env.XXMI_LSP_COMMAND;
  if (!json) {
    throw new Error('Set XXMI_LSP_COMMAND to a JSON array, e.g. ["dist/bin/xxmi-lsp","--stdio"]');
  }
  return JSON.parse(json) as string[];
}

interface Waiter {
  uri: string;
  test: (d: Diagnostic[]) => boolean;
  resolve: (d: Diagnostic[]) => void;
}

export class LspClient {
  readonly connection: ProtocolConnection;
  /** Answer of `workspace/configuration`; change it, then notify the server. */
  settings: Record<string, unknown> = { rules: {} };
  private readonly child: ChildProcessWithoutNullStreams;
  /** Messages of `window/logMessage`, in arrival order. */
  readonly logs: string[] = [];
  private readonly published = new Map<string, Diagnostic[]>();
  private readonly waiters: Waiter[] = [];

  constructor() {
    const [cmd = '', ...args] = serverCommand();
    this.child = spawn(cmd, args, { stdio: 'pipe' });
    this.child.stderr.on('data', (chunk: Buffer) => process.stderr.write(chunk));
    this.connection = createProtocolConnection(
      new StreamMessageReader(this.child.stdout),
      new StreamMessageWriter(this.child.stdin),
    );
    this.connection.onNotification(
      'textDocument/publishDiagnostics',
      (params: PublishDiagnosticsParams) => {
        this.record(params);
      },
    );
    this.connection.onNotification('window/logMessage', (params: { message: string }) => {
      this.logs.push(params.message);
    });
    this.connection.onRequest('workspace/configuration', () => [this.settings]);
    this.connection.listen();
  }

  async initialize(root: string, name: string): Promise<void> {
    await this.connection.sendRequest('initialize', {
      processId: process.pid,
      rootUri: uri(root),
      workspaceFolders: [{ uri: uri(root), name }],
      capabilities: { workspace: { configuration: true } },
    });
    await this.connection.sendNotification('initialized', {});
  }

  async open(path: string, text: string): Promise<void> {
    await this.connection.sendNotification('textDocument/didOpen', {
      textDocument: { uri: uri(path), languageId: 'migoto', version: 1, text },
    });
  }

  /**
   * Resolves with diagnostics of `target` that satisfy `test`.
   * With `fresh`, anything published before the call is ignored,
   * so a result cannot come from before the change that is being waited for.
   */
  waitForDiagnostics(
    target: string,
    test: (d: Diagnostic[]) => boolean = () => true,
    { fresh = false, timeoutMs = DEFAULT_TIMEOUT_MS } = {},
  ): Promise<Diagnostic[]> {
    if (fresh) this.published.delete(target);
    const current = this.published.get(target);
    if (current && test(current)) return Promise.resolve(current);

    return new Promise((resolve, reject) => {
      const waiter: Waiter = {
        uri: target,
        test,
        resolve: (d) => {
          clearTimeout(timer);
          resolve(d);
        },
      };
      const timer = setTimeout(() => {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        reject(new Error(`no matching diagnostics for ${target}`));
      }, timeoutMs);
      this.waiters.push(waiter);
    });
  }

  async shutdown(): Promise<void> {
    await this.connection.sendRequest('shutdown');
    await this.connection.sendNotification('exit');
    this.dispose();
  }

  dispose(): void {
    this.connection.dispose();
    this.child.kill();
  }

  private record(params: PublishDiagnosticsParams): void {
    this.published.set(params.uri, params.diagnostics);
    for (const waiter of [...this.waiters]) {
      if (waiter.uri === params.uri && waiter.test(params.diagnostics)) {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        waiter.resolve(params.diagnostics);
      }
    }
  }
}
