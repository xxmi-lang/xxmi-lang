#!/usr/bin/env node
/** `xxmi-lsp [--stdio]`: the language server over stdio (the only transport). */
import { relaunchWithWasmFlags, VERSION } from '@xxmi-lang/core';
import { createConnection, ProposedFeatures } from 'vscode-languageserver/node';
import { startServer, type ServerOptions } from './server.ts';

export function main(argv: string[], options: Omit<ServerOptions, 'version'> = {}): void {
  if (argv.includes('--version') || argv.includes('-v')) {
    console.log(VERSION);
    return;
  }
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(
      'Usage: xxmi-lsp [--stdio]\n\nLanguage server for XXMI / 3DMigoto mod .ini files (stdio).',
    );
    return;
  }
  // stdio is the only transport; editors pass --stdio, which vscode-languageserver also reads.
  if (!argv.includes('--stdio')) process.argv.push('--stdio');
  startServer(createConnection(ProposedFeatures.all), { version: VERSION, ...options });
}

if (import.meta.main && !(await relaunchWithWasmFlags())) main(process.argv.slice(2));
