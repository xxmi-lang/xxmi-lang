import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { lintPaths } from '../src/index.ts';

// fixtures/xxmi-1.3.2: syntax the DLL 1.3.2 accepts;
// expected.json files are regenerated with UPDATE_GOLDEN=1 pnpm test and reviewed.
const FIXTURES = resolve(import.meta.dirname, '../../../fixtures/xxmi-1.3.2');
const UPDATE = process.env.UPDATE_GOLDEN === '1';

async function diagnose(name: string, rules: string[]) {
  // No bundled snapshots: fixtures must not depend on ZZMI.
  const { files } = await lintPaths([join(FIXTURES, name)], { rules, snapshots: [] });
  return files.flatMap((f) =>
    f.diagnostics.map((d) => ({ id: d.id, message: d.message, range: d.range })),
  );
}

function matchesGolden(name: string, actual: unknown): void {
  const path = join(FIXTURES, name.replace(/\.ini$/, '.expected.json'));
  if (UPDATE) writeFileSync(path, `${JSON.stringify(actual, null, 2)}\n`);
  expect(actual).toEqual(JSON.parse(readFileSync(path, 'utf8')));
}

describe('DLL 1.3.2 syntax', () => {
  it('reports a misspelled [Filter] key and the match_* keys that left [Resource]', async () => {
    const actual = await diagnose('filter-keys.ini', ['XM102']);
    expect(actual.map((d) => d.id)).toEqual(['XM102', 'XM102']);
    matchesGolden('filter-keys.ini', actual);
  });

  // The grammar fork still has the 1.3.1 value-form names and no [Filter] sections, slot
  // filters or filter targets: drop `fails` once it parses them.
  it.fails('parses [Filter] sections, slot filters and filter targets', async () => {
    expect(await diagnose('pending-grammar.ini', ['XM001'])).toEqual([]);
  });
});
