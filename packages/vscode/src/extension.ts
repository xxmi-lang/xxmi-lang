/**
 * Thin LSP client (docs/08-editor-integration.md). Language features come from xxmi-lsp;
 * highlighting comes from lupomikti's "3DMigoto INI" extension (language id `migoto`).
 */
import { chmodSync } from 'node:fs';
import * as vscode from 'vscode';
import {
  LanguageClient,
  TransportKind,
  type LanguageClientOptions,
  type ServerOptions,
} from 'vscode-languageclient/node';
import { externalServer } from './external-server.ts';
import { bundledBinaryPath } from './platform.ts';

const HIGHLIGHTING_EXTENSION = 'AGMG.migoto-ini';
const HIGHLIGHTING_URL = 'https://github.com/lupomikti/migoto-vscode';

let client: LanguageClient | undefined;

function serverOptions(context: vscode.ExtensionContext): ServerOptions | undefined {
  const config = vscode.workspace.getConfiguration('xxmi');
  const external = externalServer(
    config.get<string>('lsp.path')?.trim(),
    config.get<boolean>('lsp.preferPath', false),
  );
  if (external) return { ...external, transport: TransportKind.stdio };

  const relative = bundledBinaryPath(process.platform, process.arch);
  if (!relative) {
    void vscode.window.showErrorMessage(
      `XXMI: no bundled server for ${process.platform}-${process.arch}. Set "xxmi.lsp.path" to an xxmi executable.`,
    );
    return undefined;
  }

  const command = context.asAbsolutePath(relative);
  ensureExecutable(command);
  return { command, args: ['lsp', '--stdio'], transport: TransportKind.stdio };
}

/** A VSIX extracted by some tools loses the executable bit. */
function ensureExecutable(path: string): void {
  if (process.platform === 'win32') return;
  try {
    chmodSync(path, 0o755);
  } catch {
    // Missing or read-only: the client reports the failed launch.
  }
}

async function start(context: vscode.ExtensionContext): Promise<void> {
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'migoto' }],
    synchronize: {
      configurationSection: 'xxmi',
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{ini,toml}'),
    },
  };
  const server = serverOptions(context);
  if (!server) return;

  client = new LanguageClient('xxmi', 'XXMI', server, clientOptions);
  await client.start();
}

async function suggestHighlighting(context: vscode.ExtensionContext): Promise<void> {
  const key = 'xxmi.highlightingPromptShown';
  if (vscode.extensions.getExtension(HIGHLIGHTING_EXTENSION) || context.globalState.get(key))
    return;
  await context.globalState.update(key, true);
  const choice = await vscode.window.showInformationMessage(
    'XXMI Language Tools adds diagnostics and navigation. For syntax highlighting, install lupomikti\'s "3DMigoto INI" extension.',
    'Get it',
  );
  if (choice === 'Get it') await vscode.env.openExternal(vscode.Uri.parse(HIGHLIGHTING_URL));
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand('xxmi.restartServer', async () => {
      await client?.stop();
      await start(context);
    }),
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration('xxmi.lsp.path') || e.affectsConfiguration('xxmi.lsp.preferPath'))
        await vscode.commands.executeCommand('xxmi.restartServer');
    }),
  );
  await start(context);
  void suggestHighlighting(context);
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
