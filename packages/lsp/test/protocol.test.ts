/**
 * Drives xxmi-lsp over real JSON-RPC (docs/03-lsp.md "Testing"). By default it runs the server
 * from source; set XXMI_LSP_COMMAND to a JSON array (e.g. `["./xxmi-lsp", "--stdio"]`) to test a
 * compiled binary the same way.
 */
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  createProtocolConnection,
  StreamMessageReader,
  StreamMessageWriter,
  type Diagnostic,
  type ProtocolConnection,
  type PublishDiagnosticsParams,
} from 'vscode-languageserver-protocol/node';

const PACKAGE = resolve(import.meta.dirname, '../../../fixtures/lsp/package');
const MOD = join(PACKAGE, 'Mods/MyMod');
const uri = (path: string): string => pathToFileURL(path).href;
const MOD_INI = uri(join(MOD, 'mod.ini'));
const EXTRA_INI = uri(join(MOD, 'extra.ini'));
const LIB_INI = uri(join(PACKAGE, 'Core/Lib/lib.ini'));

const command = JSON.parse(
  process.env.XXMI_LSP_COMMAND ??
    JSON.stringify([
      process.execPath,
      '--conditions=source',
      resolve(import.meta.dirname, '../src/main.ts'),
      '--stdio',
    ]),
) as string[];

let child: ChildProcessWithoutNullStreams;
let connection: ProtocolConnection;
const diagnostics = new Map<string, Diagnostic[]>();
const waiters: {
  uri: string;
  test: (d: Diagnostic[]) => boolean;
  resolve: (d: Diagnostic[]) => void;
}[] = [];

/** Resolves when the server publishes diagnostics for `uri` that satisfy `test`. */
function nextDiagnostics(
  target: string,
  test: (d: Diagnostic[]) => boolean = () => true,
): Promise<Diagnostic[]> {
  const current = diagnostics.get(target);
  if (current && test(current)) return Promise.resolve(current);
  return new Promise((resolveWait, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`no matching diagnostics for ${target}`));
    }, 10_000);
    waiters.push({
      uri: target,
      test,
      resolve: (d) => {
        clearTimeout(timer);
        resolveWait(d);
      },
    });
  });
}

beforeAll(async () => {
  const [cmd = '', ...args] = command;
  child = spawn(cmd, args, { stdio: 'pipe' });
  child.stderr.on('data', (chunk: Buffer) => process.stderr.write(chunk));
  connection = createProtocolConnection(
    new StreamMessageReader(child.stdout),
    new StreamMessageWriter(child.stdin),
  );
  connection.onNotification(
    'textDocument/publishDiagnostics',
    (params: PublishDiagnosticsParams) => {
      diagnostics.set(params.uri, params.diagnostics);
      for (const w of [...waiters]) {
        if (w.uri === params.uri && w.test(params.diagnostics)) {
          waiters.splice(waiters.indexOf(w), 1);
          w.resolve(params.diagnostics);
        }
      }
    },
  );
  connection.onRequest('workspace/configuration', () => [{ rules: {} }]);
  connection.listen();
  await connection.sendRequest('initialize', {
    processId: process.pid,
    rootUri: uri(MOD),
    workspaceFolders: [{ uri: uri(MOD), name: 'MyMod' }],
    capabilities: { workspace: { configuration: true } },
  });
  await connection.sendNotification('initialized', {});
  const text = readFileSync(join(MOD, 'mod.ini'), 'utf8');
  await connection.sendNotification('textDocument/didOpen', {
    textDocument: { uri: MOD_INI, languageId: 'migoto', version: 1, text },
  });
}, 30_000);

afterAll(async () => {
  await connection.sendRequest('shutdown');
  await connection.sendNotification('exit');
  connection.dispose();
  child.kill();
});

const pos = (line: number, character: number) => ({ line, character });

describe('xxmi-lsp over JSON-RPC', () => {
  it('publishes diagnostics for an open file (clean mod: none)', async () => {
    expect(await nextDiagnostics(MOD_INI)).toEqual([]);
  });

  it('reports XM201 after an edit introduces an unresolved run =', async () => {
    await connection.sendNotification('textDocument/didChange', {
      textDocument: { uri: MOD_INI, version: 2 },
      contentChanges: [
        { range: { start: pos(9, 0), end: pos(9, 0) }, text: 'run = CommandListLocl\n' },
      ],
    });
    const result = await nextDiagnostics(MOD_INI, (d) => d.some((x) => x.code === 'XM201'));
    const xm201 = result.find((d) => d.code === 'XM201');
    expect(xm201).toMatchObject({ source: 'xxmi', severity: 1, range: { start: pos(9, 6) } });
    expect(xm201?.message).toContain('CommandListLocl');

    // The quick fix replaces the typo.
    type Action = { title: string; edit: { changes: Record<string, { newText: string }[]> } };
    const actions = await connection.sendRequest<Action[]>('textDocument/codeAction', {
      textDocument: { uri: MOD_INI },
      range: xm201?.range,
      context: { diagnostics: [xm201] },
    });
    expect(actions[0]?.title).toBe('Change to `CommandListLocal`');
    expect(actions[0]?.edit.changes[MOD_INI]?.[0]?.newText).toBe('CommandListLocal');

    await connection.sendNotification('textDocument/didChange', {
      textDocument: { uri: MOD_INI, version: 3 },
      contentChanges: [{ range: { start: pos(9, 0), end: pos(10, 0) }, text: '' }],
    });
    expect(await nextDiagnostics(MOD_INI, (d) => d.length === 0)).toEqual([]);
  });

  it('goes to the definition in the library', async () => {
    const location = await connection.sendRequest('textDocument/definition', {
      textDocument: { uri: MOD_INI },
      position: pos(5, 10), // run = CommandList\Lib\Draw
    });
    expect(location).toEqual({ uri: LIB_INI, range: { start: pos(7, 1), end: pos(7, 16) } });
  });

  it('finds references across files of the mod', async () => {
    const locations = await connection.sendRequest<{ uri: string }[]>('textDocument/references', {
      textDocument: { uri: MOD_INI },
      position: pos(8, 5), // [CommandListLocal]
      context: { includeDeclaration: true },
    });
    expect(locations.map((l) => l.uri).sort()).toEqual([EXTRA_INI, MOD_INI, MOD_INI].sort());
  });

  it('renames a CommandList in every file, and refuses library symbols', async () => {
    const prepared = await connection.sendRequest('textDocument/prepareRename', {
      textDocument: { uri: MOD_INI },
      position: pos(8, 14),
    });
    expect(prepared).toEqual({
      range: { start: pos(8, 12), end: pos(8, 17) },
      placeholder: 'Local',
    });

    type Edit = { changes: Record<string, { range: unknown; newText: string }[]> };
    const edit = await connection.sendRequest<Edit>('textDocument/rename', {
      textDocument: { uri: MOD_INI },
      position: pos(8, 14),
      newName: 'Shared',
    });
    expect(Object.keys(edit.changes).sort()).toEqual([EXTRA_INI, MOD_INI].sort());
    expect(edit.changes[MOD_INI]).toEqual([
      { range: { start: pos(8, 12), end: pos(8, 17) }, newText: 'Shared' },
      { range: { start: pos(4, 17), end: pos(4, 22) }, newText: 'Shared' },
    ]);
    expect(edit.changes[EXTRA_INI]).toEqual([
      { range: { start: pos(4, 17), end: pos(4, 22) }, newText: 'Shared' },
    ]);

    await expect(
      connection.sendRequest('textDocument/prepareRename', {
        textDocument: { uri: MOD_INI },
        position: pos(5, 20),
      }),
    ).rejects.toThrow(/declared in the package/);
  });

  it('hovers commands and library symbols with their docs', async () => {
    const command = await connection.sendRequest<{ contents: { value: string } }>(
      'textDocument/hover',
      {
        textDocument: { uri: MOD_INI },
        position: pos(4, 1),
      },
    );
    expect(command.contents.value).toContain('**run**');

    const variable = await connection.sendRequest<{ contents: { value: string } }>(
      'textDocument/hover',
      {
        textDocument: { uri: MOD_INI },
        position: pos(6, 4),
      },
    );
    expect(variable.contents.value).toContain('Global opacity used by every mod.');
  });

  it('completes run targets with own-namespace short names', async () => {
    await connection.sendNotification('textDocument/didChange', {
      textDocument: { uri: MOD_INI, version: 4 },
      contentChanges: [{ range: { start: pos(10, 0), end: pos(10, 0) }, text: 'run = \n' }],
    });
    const items = await connection.sendRequest<{ label: string }[]>('textDocument/completion', {
      textDocument: { uri: MOD_INI },
      position: pos(10, 6),
    });
    const labels = items.map((i) => i.label);
    expect(labels).toContain('CommandListLocal');
    expect(labels).toContain('CommandList\\Lib\\Draw');
    expect(labels).toContain('BuiltInCommandListUnbindAllRenderTargets');
    await connection.sendNotification('textDocument/didChange', {
      textDocument: { uri: MOD_INI, version: 5 },
      contentChanges: [{ range: { start: pos(10, 0), end: pos(11, 0) }, text: '' }],
    });
  });

  it('outlines sections grouped by kind', async () => {
    type Symbol = { name: string; children: { name: string }[] };
    const symbols = await connection.sendRequest<Symbol[]>('textDocument/documentSymbol', {
      textDocument: { uri: MOD_INI },
    });
    expect(symbols.map((s) => [s.name, s.children.map((c) => c.name)])).toEqual([
      ['TextureOverride', ['TextureOverrideBody']],
      ['CommandList', ['CommandListLocal']],
    ]);
  });

  it('answers requests for unknown documents without failing', async () => {
    const unknown = { textDocument: { uri: uri(join(MOD, 'missing.ini')) }, position: pos(0, 0) };
    expect(await connection.sendRequest('textDocument/definition', unknown)).toBeNull();
    expect(await connection.sendRequest('textDocument/hover', unknown)).toBeNull();
  });
});
