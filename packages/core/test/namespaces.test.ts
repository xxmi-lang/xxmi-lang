import { join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { lintPaths, loadSpec, SpecLookup, Workspace } from '../src/index.ts';

// One fixture per namespace resolution rule in docs/01-language-model.md §3. Expectations are
// written by hand from the DLL's lookup order (IniHandler.cpp / CommandList.cpp), not generated.
const FIXTURES = resolve(import.meta.dirname, '../../../fixtures/namespaces');
const RESOLUTION_RULES = ['XM201', 'XM202', 'XM203', 'XM206', 'XM207'];

async function unresolved(fixture: string): Promise<string[]> {
  const dir = join(FIXTURES, fixture);
  const result = await lintPaths([dir], { rules: RESOLUTION_RULES, snapshots: [] });
  return result.files
    .flatMap((f) =>
      f.diagnostics.map((d) => {
        const ref = /`([^`]+)`|(\$\S+)/.exec(d.message);
        return `${relative(dir, f.path).split('\\').join('/')}:${d.range.start.line + 1} ${d.id} ${ref?.[1] ?? ref?.[2] ?? d.message}`;
      }),
    )
    .sort();
}

describe('namespace resolution rules', () => {
  it('1: `namespace =` names the namespace, and files can share one', async () => {
    expect(await unresolved('rule1-declared')).toEqual([]);
    const workspace = new Workspace({ lookup: new SpecLookup(loadSpec()) });
    const file = await workspace.addTarget(
      join(FIXTURES, 'rule1-declared/b.ini'),
      join(FIXTURES, 'rule1-declared'),
    );
    expect(file.namespace).toBe('Shared');
    expect(file.sections[0]?.qualifiedName).toBe('CommandList\\Shared\\B');
  });

  it('2: without a declaration, the namespace is the path from the 3DMigoto folder', async () => {
    const workspace = new Workspace({ lookup: new SpecLookup(loadSpec()) });
    const file = await workspace.addTarget(join(FIXTURES, 'rule2-path/Mods/M/m.ini'), FIXTURES);
    expect(file.namespace).toBe(join('Mods', 'M', 'm.ini').split(/[\\/]/).join('\\'));
    const main = [...workspace.files.values()].find((f) => f.path.endsWith('d3dx.ini'));
    expect(main?.namespace).toBe('');
    // The bare name only resolves inside m.ini's own namespace.
    expect(await unresolved('rule2-path')).toEqual(['Mods/Other/other.ini:3 XM201 CommandListFoo']);
  });

  it('3: local names within a namespace, `Prefix\\N\\Name` from outside', async () => {
    expect(await unresolved('rule3-qualified')).toEqual(['mod.ini:3 XM201 CommandListFoo']);
  });

  it('4: section names are opaque after the prefix (dots, spaces)', async () => {
    expect(await unresolved('rule4-opaque')).toEqual([]);
  });

  it('5: `$name` in the own namespace, `$\\N\\name` across, `local` scoped to its list', async () => {
    expect(await unresolved('rule5-variables')).toEqual([
      'mod.ini:11 XM203 $tmp',
      'mod.ini:12 XM203 $alpha',
    ]);
  });

  it('6: resource references: ref, copy, Resource\\N\\X, unless_null', async () => {
    expect(await unresolved('rule6-resources')).toEqual(['mod.ini:8 XM202 ResourceDiffuse']);
  });

  it('7: lookups are case-insensitive', async () => {
    expect(await unresolved('rule7-case')).toEqual([]);
  });
});
