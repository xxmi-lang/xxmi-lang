import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { lintPaths, RULES } from '../src/index.ts';

// fixtures/rules/XMnnn/{bad,good}(.ini|/) plus expected.json for the bad case
// (docs/02-diagnostics.md "Testing"). Regenerate with UPDATE_GOLDEN=1 pnpm test and review.
const FIXTURES = resolve(import.meta.dirname, '../../../fixtures/rules');
const UPDATE = process.env.UPDATE_GOLDEN === '1';

function variant(dir: string, name: 'bad' | 'good'): string | undefined {
  for (const candidate of [join(dir, `${name}.ini`), join(dir, name)]) {
    if (existsSync(candidate)) return candidate;
  }
  return undefined;
}

async function run(id: string, path: string) {
  // No bundled snapshots: fixtures must not depend on ZZMI.
  const result = await lintPaths([path], { rules: [id], snapshots: [] });
  const dir = join(FIXTURES, id);
  return result.files.flatMap((f) =>
    f.diagnostics.map((d) => ({
      path: relative(dir, f.path).split('\\').join('/'),
      id: d.id,
      severity: d.severity,
      message: d.message,
      range: d.range,
    })),
  );
}

describe('rule fixtures', () => {
  it('every rule has good and bad fixtures', () => {
    for (const rule of RULES) {
      const dir = join(FIXTURES, rule.id);
      expect(variant(dir, 'bad'), `${rule.id} bad`).toBeDefined();
      expect(variant(dir, 'good'), `${rule.id} good`).toBeDefined();
    }
    const dirs = readdirSync(FIXTURES).filter((d) => /^XM\d{3}$/.test(d));
    expect(dirs.sort()).toEqual(RULES.map((r) => r.id).sort());
  });

  for (const rule of RULES) {
    const dir = join(FIXTURES, rule.id);
    it(`${rule.id} reports the bad fixture exactly`, async () => {
      const bad = variant(dir, 'bad');
      if (!bad) throw new Error('missing bad fixture');
      const actual = await run(rule.id, bad);
      const expectedPath = join(dir, 'expected.json');
      if (UPDATE) writeFileSync(expectedPath, `${JSON.stringify(actual, null, 2)}\n`);
      expect(actual.length).toBeGreaterThan(0);
      expect(actual).toEqual(JSON.parse(readFileSync(expectedPath, 'utf8')));
    });

    it(`${rule.id} is quiet on the good fixture`, async () => {
      const good = variant(dir, 'good');
      if (!good) throw new Error('missing good fixture');
      expect(await run(rule.id, good)).toEqual([]);
    });
  }
});
