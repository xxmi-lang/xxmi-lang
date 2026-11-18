import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { lintPaths } from '../src/index.ts';

// fixtures/xxmi-1.3.1: syntax the DLL 1.3.1 accepts;
// expected.json files are regenerated with UPDATE_GOLDEN=1 pnpm test and reviewed.
const FIXTURES = resolve(import.meta.dirname, '../../../fixtures/xxmi-1.3.1');
const UPDATE = process.env.UPDATE_GOLDEN === '1';

async function diagnose(name: string) {
  // No bundled snapshots: fixtures must not depend on ZZMI.
  const { files } = await lintPaths([join(FIXTURES, name)], { snapshots: [] });
  return files.flatMap((f) =>
    f.diagnostics.map((d) => ({ id: d.id, message: d.message, range: d.range })),
  );
}

function matchesGolden(name: string, actual: unknown): void {
  const path = join(FIXTURES, name.replace(/\.ini$/, '.expected.json'));
  if (UPDATE) writeFileSync(path, `${JSON.stringify(actual, null, 2)}\n`);
  expect(actual).toEqual(JSON.parse(readFileSync(path, 'utf8')));
}

describe('DLL 1.3.1 syntax', () => {
  it('accepts every form of value-form run, MatchTextures and hex arguments', async () => {
    expect(await diagnose('clean.ini')).toEqual([]);
  });

  it('reports both MatchTextures operands as unresolved resources', async () => {
    const actual = await diagnose('matchtextures-missing.ini');
    expect(actual.map((d) => d.id)).toEqual(['XM202', 'XM202']);
    matchesGolden('matchtextures-missing.ini', actual);
  });

  it('still reports a misspelled value-form run as a syntax error', async () => {
    const actual = await diagnose('run-misspelled.ini');
    expect(actual.map((d) => d.id)).toEqual(['XM001']);
    matchesGolden('run-misspelled.ini', actual);
  });
});
