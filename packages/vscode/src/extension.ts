/**
 * Thin LSP client (docs/08-editor-integration.md). Language features come from xxmi-lsp;
 * highlighting comes from lupomikti's "3DMigoto INI" extension (language id `migoto`).
 */
import * as vscode from 'vscode';
import {
  LanguageClient,
  TransportKind,
  type LanguageClientOptions,
  type ServerOptions,
} from 'vscode-languageclient/node';

const HIGHLIGHTING_EXTENSION = 'AGMG.migoto-ini';
const HIGHLIGHTING_URL = 'https://github.com/lupomikti/migoto-vscode';

let client: LanguageClient | undefined;

function serverOptions(context: vscode.ExtensionContext): ServerOptions {
  const custom = vscode.workspace.getConfiguration('xxmi').get<string>('lsp.path')?.trim();
  if (custom) return { command: custom, args: ['--stdio'] };
  const module = context.asAbsolutePath('dist/server.mjs');
  // --liftoff-only: web-tree-sitter leaks native memory under V8's optimizing WASM tier
  // (see packages/core/src/runtime.ts).
  const options = { execArgv: ['--liftoff-only'] };
  return {
    run: { module, transport: TransportKind.stdio, options },
    debug: { module, transport: TransportKind.stdio, options },
  };
}

async function start(context: vscode.ExtensionContext): Promise<void> {
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'migoto' }],
    synchronize: {
      configurationSection: 'xxmi',
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{ini,toml}'),
    },
  };
  client = new LanguageClient('xxmi', 'XXMI', serverOptions(context), clientOptions);
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
      if (e.affectsConfiguration('xxmi.lsp.path'))
        await vscode.commands.executeCommand('xxmi.restartServer');
    }),
  );
  await start(context);
  void suggestHighlighting(context);
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
