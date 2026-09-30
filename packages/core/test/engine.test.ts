import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ConfigError, lintPaths, parseConfig } from '../src/index.ts';

function modDir(files: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), 'xxmi-engine-'));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
  return dir;
}

async function ids(dir: string): Promise<string[]> {
  const result = await lintPaths([dir], { snapshots: [], rules: ['XM201'] });
  return result.files.flatMap((f) =>
    f.diagnostics.map((d) => `${d.range.start.line + 1}:${d.id}:${d.severity}`),
  );
}

const MOD = ['[CommandListA]', 'run = CommandListMissing', 'run = CommandListMissing', ''].join(
  '\n',
);

describe('suppression comments', () => {
  it('disable-next-line silences one line', async () => {
    const dir = modDir({
      'mod.ini': MOD.replace('run =', '; xxmi-disable-next-line XM201\nrun ='),
    });
    expect(await ids(dir)).toEqual(['4:XM201:error']);
  });

  it('disable … enable silences a region, and a bare disable silences everything', async () => {
    const region = [
      '[CommandListA]',
      '; xxmi-disable XM201',
      'run = CommandListMissing',
      '; xxmi-enable XM201',
      'run = CommandListMissing',
      '',
    ].join('\n');
    expect(await ids(modDir({ 'mod.ini': region }))).toEqual(['5:XM201:error']);
    const all = `; xxmi-disable\n${MOD}`;
    expect(await ids(modDir({ 'mod.ini': all }))).toEqual([]);
  });

  it('only silences the named rule', async () => {
    const dir = modDir({
      'mod.ini': MOD.replace('run =', '; xxmi-disable-next-line XM999\nrun ='),
    });
    expect(await ids(dir)).toEqual(['3:XM201:error', '4:XM201:error']);
  });
});

describe('xxmi.toml', () => {
  it('turns rules off and changes severity, nearest file wins', async () => {
    const dir = modDir({ 'mod.ini': MOD, 'xxmi.toml': '[rules]\nXM201 = "warning"\n' });
    expect(await ids(dir)).toEqual(['2:XM201:warning', '3:XM201:warning']);
    const off = modDir({ 'mod.ini': MOD, 'xxmi.toml': '[rules]\nXM201 = "off"\n' });
    expect(await ids(off)).toEqual([]);
  });

  it('rejects unknown ids and settings', () => {
    expect(() => parseConfig('[rules]\nXM201 = "loud"\n', 'x.toml')).toThrow(ConfigError);
    expect(() => parseConfig('[rules]\nnope = "off"\n', 'x.toml')).toThrow(/unknown rule id/);
    expect(parseConfig('[rules.XM402]\nmin_repeats = 4\n', 'x.toml').rules).toEqual({});
  });
});
