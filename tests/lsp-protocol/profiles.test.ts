/**
 * Game profile selection in a live server (docs/01-language-model.md §5): each test starts its
 * own server on a throwaway folder, since the profile is a property of the whole workspace.
 */
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import {
  createProtocolConnection,
  StreamMessageReader,
  StreamMessageWriter,
  type Diagnostic,
  type ProtocolConnection,
  type PublishDiagnosticsParams,
} from 'vscode-languageserver-protocol/node';

const commandJson = process.env.XXMI_LSP_COMMAND;
if (!commandJson) {
  throw new Error('Set XXMI_LSP_COMMAND to a JSON array, e.g. ["dist/bin/xxmi-lsp","--stdio"]');
}
const command = JSON.parse(commandJson) as string[];

const ZZMI_REF = 'run = CommandList\\ZZMI\\SetTextures';
const WWMI_REF = 'run = CommandList\\WWMIv1\\BackupRestoreVertexTextureSlots';

const uri = (path: string): string => pathToFileURL(path).href;
// An unresolved reference is XM201, or XM206 when its namespace is not loaded.
const hasError = (d: Diagnostic[]): boolean => d.some((x) => x.severity === 1);
const clean = (d: Diagnostic[]): boolean => !hasError(d);

class Session {
  readonly dir = realpathSync(mkdtempSync(join(tmpdir(), 'xxmi-profiles-')));
  settings: Record<string, unknown> = {};
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly connection: ProtocolConnection;
  private readonly published = new Map<string, Diagnostic[]>();
  private readonly waiters: {
    uri: string;
    test: (d: Diagnostic[]) => boolean;
    done: () => void;
  }[] = [];
  private versions = new Map<string, number>();

  constructor() {
    const [cmd = '', ...args] = command;
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
    this.connection.onRequest('workspace/configuration', () => [this.settings]);
    this.connection.listen();
  }

  write(name: string, text: string): string {
    const path = join(this.dir, name);
    mkdirSync(join(path, '..'), { recursive: true });
    writeFileSync(path, text);
    return path;
  }

  async start(): Promise<void> {
    await this.connection.sendRequest('initialize', {
      processId: process.pid,
      rootUri: uri(this.dir),
      workspaceFolders: [{ uri: uri(this.dir), name: 'mods' }],
      capabilities: { workspace: { configuration: true } },
    });
    await this.connection.sendNotification('initialized', {});
  }

  async open(path: string, text: string): Promise<void> {
    this.versions.set(path, 1);
    await this.connection.sendNotification('textDocument/didOpen', {
      textDocument: { uri: uri(path), languageId: 'migoto', version: 1, text },
    });
  }

  /** Replaces the whole text of an open document. */
  async replace(path: string, text: string): Promise<void> {
    const version = (this.versions.get(path) ?? 1) + 1;
    this.versions.set(path, version);
    await this.connection.sendNotification('textDocument/didChange', {
      textDocument: { uri: uri(path), version },
      contentChanges: [{ text }],
    });
  }

  async changeSettings(settings: Record<string, unknown>): Promise<void> {
    this.settings = settings;
    await this.connection.sendNotification('workspace/didChangeConfiguration', { settings: {} });
  }

  async close(path: string): Promise<void> {
    await this.connection.sendNotification('textDocument/didClose', {
      textDocument: { uri: uri(path) },
    });
  }

  async deleted(path: string): Promise<void> {
    rmSync(path);
    await this.connection.sendNotification('workspace/didChangeWatchedFiles', {
      changes: [{ uri: uri(path), type: 3 }],
    });
  }

  /** Resolves with the next diagnostics of `path` that satisfy `test`, whatever was published before. */
  next(path: string, test: (d: Diagnostic[]) => boolean): Promise<Diagnostic[]> {
    this.published.delete(uri(path));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`no matching diagnostics for ${path}`));
      }, 3_000);
      this.waiters.push({
        uri: uri(path),
        test,
        done: () => {
          clearTimeout(timer);
          resolve(this.published.get(uri(path)) ?? []);
        },
      });
    });
  }

  stop(): void {
    this.connection.dispose();
    this.child.kill();
    rmSync(this.dir, { recursive: true, force: true });
  }

  private record(params: PublishDiagnosticsParams): void {
    this.published.set(params.uri, params.diagnostics);
    for (const waiter of [...this.waiters]) {
      if (waiter.uri === params.uri && waiter.test(params.diagnostics)) {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        waiter.done();
      }
    }
  }
}

let session: Session | undefined;
const begin = (): Session => {
  session = new Session();
  return session;
};
afterEach(() => {
  session?.stop();
  session = undefined;
});

describe('xxmi-lsp game profile', () => {
  it('honors the profile of xxmi.toml over detection', async () => {
    const s = begin();
    s.write('xxmi.toml', 'profile = "wwmi"\n');
    const path = s.write('a.ini', `[CommandListA]\n${ZZMI_REF}\n`);
    const unresolved = s.next(path, hasError);
    await s.start();
    await s.open(path, `[CommandListA]\n${ZZMI_REF}\n`);

    expect(await unresolved).toBeTruthy();
  });

  it('re-detects when the open document gains its first library reference', async () => {
    const s = begin();
    const path = s.write('a.ini', '[CommandListA]\nrun = CommandListNope\n');
    const initial = s.next(path, hasError);
    await s.start();
    await s.open(path, '[CommandListA]\nrun = CommandListNope\n');
    await initial;

    const resolved = s.next(path, clean);
    await s.replace(path, `[CommandListA]\n${ZZMI_REF}\n`);

    expect(hasError(await resolved)).toBe(false);
  });

  it('follows xxmi.miPackage as the setting changes', async () => {
    const s = begin();
    const path = s.write('a.ini', `[CommandListA]\n${ZZMI_REF}\n`);
    await s.start();
    await s.open(path, `[CommandListA]\n${ZZMI_REF}\n`);

    const forced = s.next(path, hasError);
    await s.changeSettings({ miPackage: 'wwmi' });
    await forced;

    const detected = s.next(path, clean);
    await s.changeSettings({ miPackage: 'auto' });
    expect(hasError(await detected)).toBe(false);
  });

  it('re-detects when a file with another library is opened, and when it is deleted', async () => {
    const s = begin();
    const first = s.write('a.ini', `[CommandListA]\n${WWMI_REF}\n`);
    const initial = s.next(first, clean);
    await s.start();
    await s.open(first, `[CommandListA]\n${WWMI_REF}\n`);
    await initial;

    const second = s.write('b.ini', `[CommandListB]\n${ZZMI_REF}\n`);
    const ambiguous = s.next(first, hasError);
    await s.open(second, `[CommandListB]\n${ZZMI_REF}\n`);
    await ambiguous;

    const restored = s.next(first, clean);
    await s.close(second);
    await s.deleted(second);
    expect(hasError(await restored)).toBe(false);
  });
});
