import { join, resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import {
  completions,
  defaultSpec,
  hover,
  references,
  rename,
  SpecLookup,
  targetAt,
  Workspace,
  type IniFile,
} from '../src/index.ts';

const PACKAGE = resolve(import.meta.dirname, '../../../fixtures/lsp/package');
const MOD = join(PACKAGE, 'Mods/MyMod');

let workspace: Workspace;
let mod: IniFile;

beforeAll(async () => {
  workspace = new Workspace({ lookup: new SpecLookup(defaultSpec()), snapshots: [] });
  mod = await workspace.addTarget(join(MOD, 'mod.ini'), MOD);
  await workspace.addTarget(join(MOD, 'extra.ini'), MOD);
});

const at = (line: number, character: number) => ({ line, character });

describe('variables', () => {
  it('finds a cross-namespace global from its `$\\N\\name` use', () => {
    const target = targetAt(workspace, mod, at(6, 3));
    if (!target) throw new Error('no target');
    expect(target.kind).toBe('variable');
    const refs = references(workspace, target, true);
    expect(
      refs.map((r) => [r.path.endsWith('lib.ini') ? 'lib' : 'mod', r.range.start.line]),
    ).toEqual([
      ['lib', 4],
      ['mod', 6],
    ]);
  });

  it('refuses to rename a package global from a mod', () => {
    // Lib is a package file, so renaming its global from a mod is refused...
    expect(rename(workspace, mod, at(6, 3), 'opacity')).toMatchObject({ ok: false });
  });

  it('renames a local and its uses within the command list', async () => {
    const text =
      '[CommandListA]\nlocal $tmp = 1\n$tmp = $tmp + 1\n\n[CommandListB]\nlocal $tmp = 2\n';
    const file = await workspace.openDocument(join(MOD, 'locals.ini'), text, MOD);
    const result = rename(workspace, file, at(1, 8), 'count');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.placeholder).toBe('tmp');
    const edits = result.edits
      .get(file.path)
      ?.map((e) => `${e.range.start.line}:${e.range.start.character}`);
    // Declaration on line 1 and both uses on line 2; CommandListB's own $tmp is untouched.
    expect(edits).toEqual(['1:7', '2:1', '2:8']);
    expect(rename(workspace, file, at(1, 8), '9bad')).toMatchObject({ ok: false });
    await workspace.closeDocument(join(MOD, 'locals.ini'));
  });
});

describe('hover', () => {
  it('shows the doc comment above a library section', () => {
    const result = hover(workspace, mod, at(5, 10));
    expect(result?.contents).toContain('[CommandList\\Lib\\Draw]');
    expect(result?.contents).toContain('Draws the current mesh.');
  });

  it('shows spec docs and the XXMI badge for keys', async () => {
    const file = await workspace.openDocument(
      join(MOD, 'pool.ini'),
      '[PoolSlots]\npool_size = 4\n',
      MOD,
    );
    expect(hover(workspace, file, at(1, 2))?.contents).toContain('XXMI-only');
    await workspace.closeDocument(join(MOD, 'pool.ini'));
  });
});

describe('completion contexts', () => {
  const complete = async (text: string, line: number, character: number) => {
    const file = await workspace.openDocument(join(MOD, 'scratch.ini'), text, MOD);
    const labels = completions(workspace, file, at(line, character)).map((c) => c.label);
    await workspace.closeDocument(join(MOD, 'scratch.ini'));
    return labels;
  };

  it('offers section kinds after `[`', async () => {
    expect(await complete('[Text', 0, 5)).toContain('TextureOverride');
  });

  it('offers keys and commands at the start of a command-list line', async () => {
    const labels = await complete('[TextureOverrideX]\nha', 1, 2);
    expect(labels).toContain('hash');
    expect(labels).toContain('handling');
  });

  it('offers enum values after `=`', async () => {
    expect(await complete('[ResourceX]\ntype = ', 1, 7)).toContain('StructuredBuffer');
  });

  it('offers variables after `$`, short in the own namespace', async () => {
    const labels = await complete('namespace = Lib\n[CommandListA]\n$', 2, 1);
    expect(labels).toContain('$alpha');
  });
});
