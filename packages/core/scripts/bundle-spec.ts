/**
 * Writes dist/spec.json: the merged spec (spec/generated + spec/overlay), so the published package
 * and bundled frontends (VS Code extension, single-file binaries) don't need the repo's spec/.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { loadSpec } from '../src/spec/load.ts';

const out = fileURLToPath(new URL('../dist/spec.json', import.meta.url));
mkdirSync(fileURLToPath(new URL('../dist/', import.meta.url)), { recursive: true });
writeFileSync(out, JSON.stringify(loadSpec()));
console.log(`Wrote ${out}`);
