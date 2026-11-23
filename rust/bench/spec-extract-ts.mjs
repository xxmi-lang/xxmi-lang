// Runs the TypeScript spec extractor on two source directories and writes spec/generated-style
// files; spec-parity.mjs runs it in a child process to measure time and peak memory.
//
//   node rust/bench/spec-extract-ts.mjs <dll> <baseline> <out> <dll-git-json> <baseline-git-json>

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const { extractSpec, serializeSpec } = await import(
  join(root, 'packages/spec-extractor/src/extract.ts')
);

const [dll, baseline, out, dllGit, baselineGit] = process.argv.slice(2);
const { spec } = extractSpec({
  dllRoot: dll,
  baselineRoot: baseline,
  dllGit: JSON.parse(dllGit),
  baselineGit: JSON.parse(baselineGit),
});
mkdirSync(out, { recursive: true });
for (const [name, text] of Object.entries(serializeSpec(spec))) {
  writeFileSync(join(out, name), text);
}
