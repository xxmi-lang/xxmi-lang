import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { lintPaths } from '../src/index.ts';

const SECTION = '[Resource\\R]\n';

const tempDirs: string[] = [];

// Windows removes thousands of small files slowly (AV scanning), well past vitest's 10 s hook default.
afterAll(() => {
  for (const dir of tempDirs.splice(0))
    rmSync(dir, { recursive: true, force: true, maxRetries: 3 });
}, 120_000);

function makePackage(dir: string, name: string, mods: string[]): void {
  const root = join(dir, name);
  mkdirSync(join(root, 'Mods'), { recursive: true });
  writeFileSync(join(root, 'd3dx.ini'), '[Constants]\n');
  for (const [i, text] of mods.entries()) writeFileSync(join(root, 'Mods', `mod${i}.ini`), text);
}

async function duplicateCount(dir: string): Promise<number> {
  const result = await lintPaths([dir], { rules: ['XM107'], snapshots: [] });
  return result.files.flatMap((f) => f.diagnostics).length;
}

describe('XM107 across packages', () => {
  it('does not compare sections of different packages', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'xxmi-packages-'));
    tempDirs.push(dir);
    makePackage(dir, 'p0', [SECTION]);
    makePackage(dir, 'p1', [SECTION]);
    expect(await duplicateCount(dir)).toBe(0);
  });

  it('still reports a duplicate within one package', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'xxmi-packages-'));
    tempDirs.push(dir);
    makePackage(dir, 'p0', [SECTION + SECTION]);
    makePackage(dir, 'p1', [SECTION]);
    expect(await duplicateCount(dir)).toBe(1);
  });
});
