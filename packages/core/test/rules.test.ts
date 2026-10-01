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
      ...(d.fix ? { fix: d.fix } : {}),
    })),
  );
}

type Edit = {
  range: { start: { line: number; character: number }; end: { line: number; character: number } };
  newText: string;
};

/** Applies non-overlapping edits to `text`, last first so offsets stay valid. */
function applyEdits(text: string, edits: Edit[]): string {
  const lineStarts = [0];
  for (let i = 0; i < text.length; i++) if (text[i] === '\n') lineStarts.push(i + 1);
  const offset = (p: Edit['range']['start']) => (lineStarts[p.line] ?? text.length) + p.character;
  return [...edits]
    .sort((a, b) => offset(b.range.start) - offset(a.range.start))
    .reduce(
      (out, e) => out.slice(0, offset(e.range.start)) + e.newText + out.slice(offset(e.range.end)),
      text,
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

      // docs/02-diagnostics.md: applying every quick fix to bad.ini must give fixed.ini.
      const edits = actual.flatMap((d) => d.fix?.edits ?? []);
      const fixedPath = join(dir, 'fixed.ini');
      if (edits.length > 0) {
        const fixed = applyEdits(readFileSync(bad, 'utf8'), edits);
        if (UPDATE) writeFileSync(fixedPath, fixed);
        expect(fixed).toBe(readFileSync(fixedPath, 'utf8'));
      } else {
        expect(existsSync(fixedPath), `${fixedPath} without quick fixes`).toBe(false);
      }
    });

    it(`${rule.id} is quiet on the good fixture`, async () => {
      const good = variant(dir, 'good');
      if (!good) throw new Error('missing good fixture');
      expect(await run(rule.id, good)).toEqual([]);
    });
  }
});
