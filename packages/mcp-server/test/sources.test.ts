import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { keepPath } from '../src/extract.ts';
import { readExcerpt, safeJoin, searchSource, SourceError } from '../src/sources.ts';
import { FIXTURE_DIRS, tempDir } from './helpers.ts';

const hasRipgrep = ((): boolean => {
  try {
    execFileSync('rg', ['--version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
})();

describe('safeJoin', () => {
  const root = tempDir();

  it('accepts paths inside the root, with either separator', () => {
    expect(safeJoin(root, 'a/b.ini')).toBe(join(root, 'a', 'b.ini'));
    expect(safeJoin(root, 'a\\b.ini')).toBe(join(root, 'a', 'b.ini'));
  });

  it('rejects traversal, absolute paths, drive letters, NUL and the root itself', () => {
    for (const bad of [
      '../x',
      'a/../../x',
      '/etc/passwd',
      'C:\\x',
      'C:/x',
      'a\0b',
      '',
      '.',
      'a/..',
    ]) {
      expect(safeJoin(root, bad), JSON.stringify(bad)).toBeUndefined();
    }
  });
});

describe('readExcerpt', () => {
  it('numbers lines and clamps to the end of the file', () => {
    const root = tempDir();
    writeFileSync(join(root, 'f.txt'), 'one\ntwo\nthree');

    const excerpt = readExcerpt(root, 'f.txt', 2, 10);

    expect(excerpt).toMatchObject({ startLine: 2, endLine: 3, totalLines: 3 });
    expect(excerpt.text).toBe('2| two\n3| three');
  });

  it('refuses more than 400 lines, inverted ranges and missing files', () => {
    const root = tempDir();
    writeFileSync(join(root, 'f.txt'), 'x');

    expect(() => readExcerpt(root, 'f.txt', 1, 401)).toThrow(SourceError);
    expect(() => readExcerpt(root, 'f.txt', 5, 2)).toThrow(SourceError);
    expect(() => readExcerpt(root, 'nope.txt', 1, 2)).toThrow(/no such file/);
    mkdirSync(join(root, 'dir'));
    expect(() => readExcerpt(root, 'dir', 1, 2)).toThrow(/no such file/);
  });
});

describe('searchSource', () => {
  it.skipIf(!hasRipgrep)('finds matches with context and honors the result cap', async () => {
    const found = await searchSource(FIXTURE_DIRS['dll'] ?? '', 'L"store"', {
      maxResults: 10,
      timeoutMs: 2000,
    });
    const capped = await searchSource(FIXTURE_DIRS['dll'] ?? '', 'L"store"', {
      maxResults: 1,
      timeoutMs: 2000,
    });

    expect(found.matches.map((m) => `${m.path}:${m.line}`)).toEqual([
      'DirectX11/CommandList.cpp:5',
      'DirectX11/CommandList.cpp:11',
    ]);
    expect(found.matches[0]?.context.length).toBeGreaterThan(0);
    expect(capped.matches).toHaveLength(1);
    expect(capped.truncated).toBe(true);
  });

  it.skipIf(!hasRipgrep)('applies the path glob and reports invalid regexes', async () => {
    const dll = FIXTURE_DIRS['dll'] ?? '';

    const none = await searchSource(dll, 'store', {
      pathGlob: '*.md',
      maxResults: 5,
      timeoutMs: 2000,
    });
    expect(none.matches).toEqual([]);
    await expect(searchSource(dll, '(', { maxResults: 5, timeoutMs: 2000 })).rejects.toThrow(
      /invalid search/,
    );
  });

  it.skipIf(!hasRipgrep)('is not hurt by a catastrophic-backtracking pattern', async () => {
    const root = tempDir();
    writeFileSync(join(root, 'a.txt'), `${'a'.repeat(5000)}!\n`);

    const started = Date.now();
    await searchSource(root, '(a+)+$', { maxResults: 5, timeoutMs: 2000 });

    expect(Date.now() - started).toBeLessThan(2500);
  });
});

describe('keepPath', () => {
  it('keeps text sources, drops binaries and third-party DLL folders', () => {
    expect(keepPath('package', 'ZZMI/Core/a.ini')).toBe(true);
    expect(keepPath('package', 'ZZMI/Textures/a.dds')).toBe(false);
    expect(keepPath('dll', 'DirectX11/CommandList.cpp')).toBe(true);
    expect(keepPath('dll', 'pcre2/src/big.c')).toBe(false);
    expect(keepPath('dll', 'README.md')).toBe(true);
    expect(keepPath('tools', 'a/../b.py')).toBe(false);
  });
});
