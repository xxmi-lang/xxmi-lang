/**
 * Regenerates spec/generated/*.json from DLL source checkouts.
 *
 *   XXMI_DLL_SRC=<XXMI-Libs-Package> MIGOTO_DLL_SRC=<bo3b/3Dmigoto> pnpm spec:extract [--check]
 *
 * --dll DIR / --baseline DIR / --out DIR override the environment and default output.
 * --check  don't write; exit 1 if the output would differ from what's on disk.
 *
 * Exit codes: 0 ok, 1 --check found a difference, 2 usage error, 3 extraction failed.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { AnchorNotFoundError } from './cpp.ts';
import { extractSpec, serializeSpec } from './extract.ts';

const REPO = resolve(import.meta.dirname, '../../..');

const { values } = parseArgs({
  options: {
    dll: { type: 'string' },
    baseline: { type: 'string' },
    out: { type: 'string' },
    check: { type: 'boolean', default: false },
  },
});

const dllRoot = values.dll ?? process.env.XXMI_DLL_SRC;
const baselineRoot = values.baseline ?? process.env.MIGOTO_DLL_SRC;
const out = resolve(values.out ?? join(REPO, 'spec/generated'));
if (!dllRoot || !baselineRoot) {
  console.error(
    'usage: XXMI_DLL_SRC=<XXMI-Libs-Package> MIGOTO_DLL_SRC=<3Dmigoto> pnpm spec:extract [--check]',
  );
  process.exit(2);
}

let files: Record<string, string>;
try {
  const { spec, baselineWarnings } = extractSpec({ dllRoot, baselineRoot });
  for (const w of baselineWarnings) console.error(`baseline (not XXMI): ${w}`);
  files = serializeSpec(spec);
  console.error(`Extracted from ${spec.meta.dll.repository || dllRoot} @ ${spec.meta.dll.commit}`);
} catch (error) {
  if (error instanceof AnchorNotFoundError) {
    console.error(`Extraction failed: ${error.message}`);
    console.error('The DLL source moved. Update the anchor in packages/spec-extractor/src/.');
    process.exit(3);
  }
  throw error;
}

if (values.check) {
  const stale = Object.entries(files).filter(([name, text]) => {
    const path = join(out, name);
    return !existsSync(path) || readFileSync(path, 'utf8') !== text;
  });
  for (const [name] of stale) console.error(`differs: ${join(out, name)}`);
  process.exit(stale.length > 0 ? 1 : 0);
}

mkdirSync(out, { recursive: true });
for (const [name, text] of Object.entries(files)) writeFileSync(join(out, name), text);
console.error(`Wrote ${Object.keys(files).length} files to ${out}`);
