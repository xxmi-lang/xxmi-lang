import { describe, expect, it } from 'vitest';
import { parse as parseToml } from 'smol-toml';
import { applyOverlay, loadGeneratedSpec, loadSpec, type Spec } from '../src/index.ts';

const generated = loadGeneratedSpec();
const fresh = (): Spec => structuredClone(generated);

function overlay(toml: string): { spec: Spec; problems: string[] } {
  const spec = fresh();
  return { spec, problems: applyOverlay(spec, parseToml(toml), 'test.toml') };
}

describe('spec/ in the repository', () => {
  it('loads with every overlay applied', () => {
    const spec = loadSpec();
    expect(spec.commands.find((c) => c.name === 'store')?.doc).toMatch(/buffer/);
    const resource = spec.sections.find((s) => s.name === 'Resource');
    expect(resource?.keys.find((k) => k.name === 'type')?.doc).toMatch(/StructuredBuffer/);
  });

  it('has a source and an xxmi flag on every generated entry', () => {
    const entries: { source: string; xxmi: boolean }[] = [
      ...generated.sections,
      ...generated.sections.flatMap((s) => [...s.keys, ...s.dynamicKeys]),
      ...generated.commands,
      ...generated.commands.flatMap((c) => c.values),
      ...generated.resourceMembers,
      ...generated.functions,
      ...generated.operators.tokens,
      ...generated.operators.precedence,
      ...generated.enums,
      ...generated.enums.flatMap((e) => e.values),
    ];
    for (const e of entries) {
      expect(e.source).toMatch(/^[\w./]+:\w[\w:]*$/);
      expect(typeof e.xxmi).toBe('boolean');
    }
  });

  it('records the DLL commit it was extracted from', () => {
    expect(generated.meta.dll.commit).toMatch(/^[0-9a-f]{40}$/);
    expect(generated.meta.baseline.commit).toMatch(/^[0-9a-f]{40}$/);
  });
});

describe('applyOverlay', () => {
  it('adds docs to existing entries, case-insensitively', () => {
    const { spec, problems } = overlay(`
      [commands.STORE]
      doc = "d"
      example = "e"
      [sections.resource.keys.TYPE]
      doc = "k"
      valueType = "enum:CustomResourceTypeNames"
      [operators."<<"]
      doc = "shift"
      [members.Region]
      args = ["unsigned", "unsigned"]
    `);
    expect(problems).toEqual([]);
    expect(spec.commands.find((c) => c.name === 'store')).toMatchObject({ doc: 'd', example: 'e' });
    expect(
      spec.sections.find((s) => s.name === 'Resource')?.keys.find((k) => k.name === 'type')?.doc,
    ).toBe('k');
    expect(spec.operators.tokens.find((t) => t.token === '<<')?.doc).toBe('shift');
  });

  it('rejects entries that are not in the generated spec', () => {
    const { problems } = overlay(`
      [commands.not_a_command]
      doc = "x"
    `);
    expect(problems).toEqual([
      'test.toml: commands.not_a_command: not in the generated spec (add `source = "manual"` and a `note` if intended)',
    ]);
  });

  it('adds manual entries only with a note and required fields', () => {
    const ok = overlay(`
      [sections.Resource.keys.hand_made]
      source = "manual"
      note = "read through a dynamic key name the extractor can't see"
      valueType = "int"
      doc = "d"
    `);
    expect(ok.problems).toEqual([]);
    expect(ok.spec.sections.find((s) => s.name === 'Resource')?.keys.at(-1)).toEqual({
      name: 'hand_made',
      valueType: 'int',
      source: 'manual',
      xxmi: false,
      doc: 'd',
      note: "read through a dynamic key name the extractor can't see",
    });

    const bad = overlay(`
      [commands.hand_made]
      source = "manual"
    `);
    expect(bad.problems).toEqual([
      'test.toml: commands.hand_made: manual entries need a `note` saying why they exist',
      'test.toml: commands.hand_made: manual entries of this kind need `kind`',
    ]);
  });

  it('rejects fields an overlay may not set', () => {
    const { problems } = overlay(`
      [commands.store]
      xxmi = false
      source = "manual"
      doc = 3
      [unknown_group.x]
      doc = "y"
    `);
    expect(problems).toEqual([
      "test.toml: commands.store: field `xxmi` can't be set by an overlay",
      'test.toml: commands.store: `source` can only be set on new (`source = "manual"`) entries',
      'test.toml: commands.store: `doc` must be a string',
      'test.toml: unknown_group: unknown overlay group',
    ]);
  });
});
