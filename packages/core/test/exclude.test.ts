import { mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { defaultSpec, globToRegExp, lintPaths, SpecLookup, Workspace } from '../src/index.ts';

// `exclude_recursive` (IniHandler.cpp ParseIniFilesRecursive): the DLL skips matching file and
// folder names while walking `include_recursive` folders, so those files are neither linted in
// bulk nor duplicates of the files it does load.
const PACKAGE = resolve(import.meta.dirname, '../../../fixtures/exclude');
const DISABLED = join(PACKAGE, 'Mods/Test/DISABLED_old/Lib/lib.ini');

async function duplicates(path: string): Promise<string[]> {
  const result = await lintPaths([path], { rules: ['XM107'], snapshots: [] });
  return result.files.flatMap((f) =>
    f.diagnostics.map((d) => `${relative(PACKAGE, f.path).split('\\').join('/')} ${d.id}`),
  );
}

describe('exclude_recursive', () => {
  it('matches names like PCRE2 globs, ignoring case', () => {
    expect(globToRegExp('DISABLED*').test('disabled_old')).toBe(true);
    expect(globToRegExp('DISABLED*').test('MyDISABLED')).toBe(false);
    expect(globToRegExp('desktop.ini').test('Desktop.ini')).toBe(true);
    expect(globToRegExp('desktop.ini').test('desktopXini')).toBe(false);
    expect(globToRegExp('mod?.ini').test('mod1.ini')).toBe(true);
    expect(globToRegExp('[ab]*').test('b-side')).toBe(true);
  });

  it('skips excluded folders when linting a folder', async () => {
    const result = await lintPaths([join(PACKAGE, 'Mods')], { rules: [], snapshots: [] });
    const paths = result.files.map((f) => relative(PACKAGE, f.path).split('\\').join('/'));
    expect(paths.sort()).toEqual(['Mods/Test/Mod/copy.ini', 'Mods/Test/Mod/mod.ini']);
  });

  it('does not report a disabled copy of a library as a duplicate', async () => {
    expect(await duplicates(DISABLED)).toEqual([]);
    expect(await duplicates(join(PACKAGE, 'Core/Lib/lib.ini'))).toEqual([]);
  });

  it('still reports an enabled copy', async () => {
    expect(await duplicates(join(PACKAGE, 'Mods/Test/Mod/copy.ini'))).toEqual([
      'Mods/Test/Mod/copy.ini XM107',
    ]);
  });
});

describe('symlinked files', () => {
  it('follow the editor buffer of any of their paths', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'xxmi-alias-'));
    mkdirSync(join(dir, 'Core'));
    mkdirSync(join(dir, 'Work'));
    writeFileSync(join(dir, 'd3dx.ini'), '[Include]\ninclude = Core\\lib.ini\n');
    writeFileSync(join(dir, 'Work/lib.ini'), 'namespace = Lib\n[CommandListOld]\n');
    try {
      symlinkSync(join(dir, 'Work/lib.ini'), join(dir, 'Core/lib.ini'));
    } catch {
      return; // no symlink permission (Windows without developer mode)
    }
    const workspace = new Workspace({ lookup: new SpecLookup(defaultSpec()), snapshots: [] });
    await workspace.loadPackage(dir);
    const linked = workspace.files.get(join(dir, 'Core/lib.ini'));
    expect(linked?.sections.map((s) => s.name.text)).toEqual(['CommandListOld']);

    await workspace.openDocument(
      join(dir, 'Work/lib.ini'),
      'namespace = Lib\n[CommandListNew]\n',
      dir,
    );
    const updated = workspace.files.get(join(dir, 'Core/lib.ini'));
    expect(updated?.sections.map((s) => s.name.text)).toEqual(['CommandListNew']);

    await workspace.closeDocument(join(dir, 'Work/lib.ini'));
    const closed = workspace.files.get(join(dir, 'Core/lib.ini'));
    expect(closed?.sections.map((s) => s.name.text)).toEqual(['CommandListOld']);
  });
});
