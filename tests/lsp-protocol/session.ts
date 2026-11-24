import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import type { Diagnostic } from 'vscode-languageserver-protocol/node';
import { LspClient, uri } from './client.ts';

const FileChangeType = { Created: 1, Deleted: 3 };

/** A throwaway workspace folder with its own server, for tests that need a fresh workspace. */
export class Session {
  readonly dir = realpathSync(mkdtempSync(join(tmpdir(), 'xxmi-lsp-')));
  readonly client = new LspClient();
  private readonly versions = new Map<string, number>();

  write(name: string, text: string): string {
    const path = join(this.dir, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text);
    return path;
  }

  start(): Promise<void> {
    return this.client.initialize(this.dir, 'mods');
  }

  async open(path: string, text: string): Promise<void> {
    this.versions.set(path, 1);
    await this.client.open(path, text);
  }

  /** Replaces the whole text of an open document. */
  async replace(path: string, text: string): Promise<void> {
    const version = (this.versions.get(path) ?? 1) + 1;
    this.versions.set(path, version);
    await this.client.connection.sendNotification('textDocument/didChange', {
      textDocument: { uri: uri(path), version },
      contentChanges: [{ text }],
    });
  }

  async changeSettings(settings: Record<string, unknown>): Promise<void> {
    this.client.settings = settings;
    await this.client.connection.sendNotification('workspace/didChangeConfiguration', {
      settings: {},
    });
  }

  async close(path: string): Promise<void> {
    await this.client.connection.sendNotification('textDocument/didClose', {
      textDocument: { uri: uri(path) },
    });
  }

  async deleted(path: string): Promise<void> {
    rmSync(path);
    await this.watched([path], FileChangeType.Deleted);
  }

  /** Reports files already written to disk as created, in one notification. */
  created(...paths: string[]): Promise<void> {
    return this.watched(paths, FileChangeType.Created);
  }

  private async watched(paths: string[], type: number): Promise<void> {
    await this.client.connection.sendNotification('workspace/didChangeWatchedFiles', {
      changes: paths.map((path) => ({ uri: uri(path), type })),
    });
  }

  /** Diagnostics of `path` that satisfy `test`, published after this call. */
  next(path: string, test: (d: Diagnostic[]) => boolean): Promise<Diagnostic[]> {
    return this.client.waitForDiagnostics(uri(path), test, { fresh: true });
  }

  stop(): void {
    this.client.dispose();
    rmSync(this.dir, { recursive: true, force: true });
  }
}
