// Runs inside VS Code's extension host (loaded by @vscode/test-electron), so it uses the real
// extension, the bundled server and VS Code's own provider commands, like the UI does.
const assert = require('node:assert/strict');
const path = require('node:path');
const vscode = require('vscode');

const MOD = path.resolve(__dirname, '../../../fixtures/lsp/package/Mods/MyMod/mod.ini');

/** Polls `get` until `ok` holds (the server starts and indexes asynchronously). */
async function waitFor(what, get, ok, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    last = await get();
    if (ok(last)) return last;
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`timed out waiting for ${what}; last value: ${JSON.stringify(last)}`);
}

const code = (d) => (typeof d.code === 'object' ? d.code.value : d.code);

exports.run = async () => {
  const doc = await vscode.workspace.openTextDocument(MOD);
  await vscode.window.showTextDocument(doc);
  assert.equal(doc.languageId, 'migoto');

  // Go to definition into the library (Core/Lib/lib.ini, [CommandListDraw]).
  const defs = await waitFor(
    'definition',
    () =>
      vscode.commands.executeCommand(
        'vscode.executeDefinitionProvider',
        doc.uri,
        new vscode.Position(5, 10),
      ),
    (r) => Array.isArray(r) && r.length > 0,
  );
  const def = defs[0];
  const target = def.targetUri ?? def.uri;
  const range = def.targetRange ?? def.range;
  assert.ok(target.fsPath.endsWith(path.join('Core', 'Lib', 'lib.ini')), target.fsPath);
  assert.equal(range.start.line, 7);

  // Rename a CommandList: edits in both files of the mod.
  const rename = await vscode.commands.executeCommand(
    'vscode.executeDocumentRenameProvider',
    doc.uri,
    new vscode.Position(8, 14),
    'Shared',
  );
  const files = rename
    .entries()
    .map(([uri]) => path.basename(uri.fsPath))
    .sort();
  assert.deepEqual(files, ['extra.ini', 'mod.ini']);

  // Hover docs for a command.
  const hovers = await vscode.commands.executeCommand(
    'vscode.executeHoverProvider',
    doc.uri,
    new vscode.Position(4, 1),
  );
  const hoverText = hovers
    .flatMap((h) => h.contents.map((c) => (typeof c === 'string' ? c : c.value)))
    .join('\n');
  assert.match(hoverText, /\*\*run\*\*/);

  // An edit that introduces an unresolved run = produces XM201.
  const edit = new vscode.WorkspaceEdit();
  edit.insert(doc.uri, new vscode.Position(9, 0), 'run = CommandListLocl\n');
  assert.ok(await vscode.workspace.applyEdit(edit));
  const diagnostics = await waitFor(
    'XM201',
    () => vscode.languages.getDiagnostics(doc.uri),
    (d) => d.some((x) => code(x) === 'XM201'),
  );
  assert.equal(diagnostics.find((x) => code(x) === 'XM201').range.start.line, 9);
  await vscode.commands.executeCommand('workbench.action.files.revert');
};
