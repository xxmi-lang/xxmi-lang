/**
 * The bundled server: xxmi-lsp with its data loaded from files shipped next to this bundle
 * (dist/), so the extension needs no separate install.
 */
import { readFileSync } from 'node:fs';
import { configureDefaults, initParser, type Spec, type SymbolSnapshot } from '@xxmi-lang/core';
import { startServer, VERSION } from '@xxmi-lang/lsp';
import { createConnection, ProposedFeatures } from 'vscode-languageserver/node';

const read = (name: string): Buffer => readFileSync(new URL(`./${name}`, import.meta.url));

configureDefaults({
  spec: JSON.parse(read('spec.json').toString('utf8')) as Spec,
  snapshots: [JSON.parse(read('zzmi.json').toString('utf8')) as SymbolSnapshot],
});
await initParser({
  grammarWasm: read('tree-sitter-migoto.wasm'),
  runtimeWasm: read('web-tree-sitter.wasm'),
});
startServer(createConnection(ProposedFeatures.all), { version: VERSION });
