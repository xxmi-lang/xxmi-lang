/**
 * Writes dist/spec.json: the merged spec (spec/generated + spec/overlay), so the published package
 * and bundled frontends (VS Code extension, single-file binaries) don't need the repo's spec/.
 * dist/overlay.json holds the parsed overlays alone, for servers that extract specs at runtime.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { loadSpec, readOverlays } from '../src/spec/load.ts';

const out = fileURLToPath(new URL('../dist/spec.json', import.meta.url));
mkdirSync(fileURLToPath(new URL('../dist/', import.meta.url)), { recursive: true });
writeFileSync(out, JSON.stringify(loadSpec()));
console.log(`Wrote ${out}`);

const overlayOut = fileURLToPath(new URL('../dist/overlay.json', import.meta.url));
writeFileSync(overlayOut, JSON.stringify(readOverlays().overlays));
console.log(`Wrote ${overlayOut}`);
