import { resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { lintText } from '../src/index.ts';

// fixtures/ownership/mixed.ini: every line a dedicated rule names is reported by that rule alone,
// not also as XM001, XM102, XM103, XM109, XM110 or XM204. The Rust core checks the same list.
const DIR = resolve(import.meta.dirname, '../../../fixtures/ownership');

describe('rule ownership', () => {
  it('reports each stray line once, under its own rule', async () => {
    const text = readFileSync(resolve(DIR, 'mixed.ini'), 'utf8');
    const result = await lintText(resolve(DIR, 'mixed.ini'), text, { snapshots: [] });
    const found = result.files.flatMap((f) =>
      f.diagnostics
        .filter((d) => /^XM0|^XM10[239]$|^XM11[01]$|^XM20[49]$|^XM21[01]$|^XM403$/.test(d.id))
        .map((d) => `${d.range.start.line + 1}:${d.id}`),
    );
    expect(found).toEqual(JSON.parse(readFileSync(resolve(DIR, 'expected.json'), 'utf8')));
  });
});
