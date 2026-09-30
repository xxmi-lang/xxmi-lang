import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import type { Spec } from '@xxmi-lang/core';
import { describe, expect, it } from 'vitest';
import { AnchorNotFoundError, CppFile } from '../src/cpp.ts';
import { DllSource } from '../src/dll.ts';
import { extractBody, extractSpec, serializeSpec } from '../src/extract.ts';

const FIXTURES = resolve(import.meta.dirname, '../../../fixtures/spec-extractor');
const EXPECTED = join(FIXTURES, 'expected');
const UPDATE = process.env.UPDATE_GOLDEN === '1';

const git = (name: string) => ({
  repository: `https://example.invalid/${name}`,
  commit: '0000000000000000000000000000000000000000',
  commitDate: '2026-01-01T00:00:00Z',
});

function extractFixture() {
  return extractSpec({
    dllRoot: join(FIXTURES, 'xxmi'),
    baselineRoot: join(FIXTURES, 'vanilla'),
    dllGit: git('xxmi'),
    baselineGit: git('vanilla'),
  });
}

const flag = (
  entries: { name?: string; token?: string; xxmi: boolean }[],
  name: string,
): boolean | undefined =>
  entries.find((e) => (e.name ?? e.token)?.toLowerCase() === name.toLowerCase())?.xxmi;

describe('extractSpec on the synthetic DLL pair', () => {
  const { spec, baselineWarnings } = extractFixture();

  it('matches the golden output', () => {
    const files = serializeSpec(spec);
    if (UPDATE) {
      mkdirSync(EXPECTED, { recursive: true });
      for (const [name, text] of Object.entries(files)) writeFileSync(join(EXPECTED, name), text);
    }
    for (const [name, text] of Object.entries(files)) {
      expect(text, name).toBe(readFileSync(join(EXPECTED, name), 'utf8'));
    }
  });

  it('is byte-identical across runs', () => {
    expect(serializeSpec(extractFixture().spec)).toEqual(serializeSpec(spec));
  });

  it('gives every entry a source', () => {
    const sources: string[] = [];
    for (const s of spec.sections) sources.push(s.source, ...s.keys.map((k) => k.source));
    sources.push(
      ...spec.commands.map((c) => c.source),
      ...spec.operators.tokens.map((t) => t.source),
    );
    sources.push(
      ...spec.operators.precedence.map((p) => p.source),
      ...spec.enums.map((e) => e.source),
    );
    expect(sources.every((s) => /^[\w./]+:\w[\w:]*$/.test(s))).toBe(true);
  });

  it('flags only what the vanilla baseline lacks', () => {
    expect(flag(spec.sections, 'Pool')).toBe(true);
    expect(flag(spec.sections, 'Resource')).toBe(false);
    const resource = spec.sections.find((s) => s.name === 'Resource');
    expect(flag(resource?.keys ?? [], 'color_space')).toBe(true);
    expect(flag(resource?.keys ?? [], 'type')).toBe(false);
    expect(flag(spec.commands, 'store')).toBe(true);
    expect(flag(spec.commands, 'commandlist')).toBe(true);
    expect(flag(spec.commands, 'locked')).toBe(true);
    expect(flag(spec.commands, 'run')).toBe(false);
    expect(flag(spec.commands, 'global')).toBe(false);
    // Vanilla writes `name.compare(0, 6, L"local ")`, XXMI `line.compare(0, 5, L"local")`.
    expect(flag(spec.commands, 'local')).toBe(false);
    expect(flag(spec.operators.tokens, '<<')).toBe(true);
    expect(flag(spec.operators.tokens, '&')).toBe(true);
    expect(flag(spec.operators.tokens, '//')).toBe(false);
    expect(spec.operators.precedence.find((p) => p.group === 'shift_operators')?.xxmi).toBe(true);
    expect(spec.resourceMembers.every((m) => m.xxmi)).toBe(true);
    expect(flag(spec.functions, 'sin')).toBe(true);
    expect(flag(spec.enums, 'PoolIndexTypeNames')).toBe(true);
  });

  it('reports the anchors the baseline lacks', () => {
    expect(baselineWarnings).toEqual([
      'CommandList.cpp: could not find function ResourceCopyTarget::ParseTargetMember',
    ]);
  });

  it('extracts keys from whitelists, getters, comparisons and literal sections', () => {
    const section = (name: string) => spec.sections.find((s) => s.name === name);
    const keys = (name: string) => section(name)?.keys.map((k) => `${k.name}:${k.valueType}`);
    expect(keys('TextureOverride')).toEqual([
      'hash:hash',
      'match_priority:int',
      'override_byte_stride:int',
      'match_type:enum:ResourceDimensionNames',
      'match_format:enum:DXGIFormats',
    ]);
    expect(keys('Resource')).toEqual([
      'max_copies_per_frame:int',
      'filename:string',
      'type:enum:CustomResourceTypeNames',
      'format:enum:DXGIFormats',
      'color_space:enum:CustomColorSpaceNames',
      'width:int',
      'data:string',
    ]);
    expect(keys('Include')).toEqual([
      'include:string',
      'include_recursive:string',
      'user_config:string',
    ]);
    expect(keys('Logging')).toEqual(['log_level:enum:LogVerbosityNames', 'debug:bool']);
    expect(keys('Loader')).toEqual([
      'loader:string',
      'check_version:bool',
      'module:string',
      'require_admin:bool',
      'delay:int',
    ]);
    expect(keys('Rendering')).toEqual(['storage_directory:string']);
    expect(section('Resource')?.removedKeys.map((k) => k.name)).toEqual(['mode']);
    expect(section('Logging')?.removedKeys).toEqual([]);
    expect(
      section('Key')
        ?.keys.filter((k) => k.repeatable)
        .map((k) => k.name),
    ).toEqual(['Key', 'Back']);
    expect(section('Key')?.dynamicKeys.map((d) => d.kind)).toEqual(['ini_param', 'variable']);
    expect(section('Include')?.allowsDuplicateKeys).toBe(true);
    expect(section('CommandList')).toMatchObject({
      allowsBareLines: true,
      allowsDuplicateKeys: true,
      keys: [],
    });
    expect(section('Resource')).toMatchObject({
      allowsBareLines: false,
      allowsDuplicateKeys: false,
    });
  });

  it('extracts commands with values, argument counts and precedence', () => {
    const cmd = (name: string) => spec.commands.find((c) => c.name === name);
    expect(cmd('run')?.values.map((v) => `${v.name}/${v.match}`)).toEqual([
      'customshader/prefix',
      'commandlist/prefix',
    ]);
    expect(cmd('drawindexed')).toMatchObject({ kind: 'draw', argCount: 3 });
    expect(cmd('store')).toMatchObject({ kind: 'general', argCount: 3 });
    expect(cmd('handling')?.argCount).toBeUndefined();
    expect(spec.builtinSections.map((b) => b.name)).toEqual([
      'BuiltInCommandListUnbindAllRenderTargets',
    ]);
    expect(cmd('dispatchindirect')).toMatchObject({ argCount: 1, indirect: true });
    expect(cmd('if')).toMatchObject({ kind: 'flow', match: 'prefix' });
    expect(spec.commands.filter((c) => c.kind === 'prefix').map((c) => c.name)).toEqual([
      'post',
      'pre',
    ]);
    expect(spec.resourceMembers.map((m) => `${m.name}(${m.args.join(',')})`)).toEqual([
      'size()',
      'region(unsigned,unsigned)',
    ]);
    expect(spec.operators.precedence.map((p) => `${p.group}:${p.operators.join(' ')}`)).toEqual([
      'unary_operators:! -',
      'multi_division_operators:* //',
      'add_subtract_operators:+',
      'shift_operators:<< >>',
      'bitwise_and_operators:&',
      'and_operators:&&',
    ]);
    expect(spec.operators.precedence[0]).toMatchObject({ associativity: 'right', unary: true });
  });
});

describe('source drift', () => {
  function withEdit(file: string, edit: (text: string) => string): DllSource {
    const dll = DllSource.load(join(FIXTURES, 'xxmi'));
    return new DllSource(
      dll.all().map((f) => (f.name === file ? new CppFile(f.name, edit(f.text)) : f)),
    );
  }

  it('fails loudly when a parser function is renamed', () => {
    const dll = withEdit('CommandList.cpp', (t) =>
      t.replace('ParseCommandListFlowControl(', 'ParseFlowControlRenamed('),
    );
    expect(() => extractBody(dll, true)).toThrow(AnchorNotFoundError);
    expect(() => extractBody(dll, true)).toThrow(/ParseCommandListFlowControl/);
  });

  it('fails loudly when a table is renamed', () => {
    const dll = withEdit('IniHandler.cpp', (t) =>
      t.replace('RegularSections[]', 'OtherSections[]'),
    );
    expect(() => extractBody(dll, true)).toThrow(/RegularSections/);
  });

  it('fails loudly when a macro in a key list is missing', () => {
    const dll = withEdit('IniHandler.cpp', (t) =>
      t.replace('#define TEXTURE_OVERRIDE_FUZZY_MATCHES', '#define SOMETHING_ELSE'),
    );
    expect(() => extractBody(dll, true)).toThrow(/TEXTURE_OVERRIDE_FUZZY_MATCHES/);
  });

  it('fails loudly when a component folder is missing (e.g. a sparse checkout)', () => {
    const full = DllSource.load(join(FIXTURES, 'xxmi'));
    const partial = new DllSource(
      full.all().filter((f) => !f.name.startsWith('Injector/')),
      ['Injector'],
    );
    expect(() => extractBody(partial, true)).toThrow(/Injector\/: could not find directory/);
    expect(extractBody(partial, false).warnings).toHaveLength(1);
  });

  it('records instead of failing in baseline mode', () => {
    const dll = withEdit('CommandList.cpp', (t) =>
      t.replace('ResourceCopyTarget::ParseTargetMember', 'Renamed::Member'),
    );
    const { body, warnings } = extractBody(dll, false);
    expect(body.resourceMembers).toEqual([]);
    expect(warnings).toHaveLength(1);
  });
});

// Runs only with real checkouts: XXMI_DLL_SRC (XXMI-Libs-Package) and MIGOTO_DLL_SRC (3Dmigoto).
const dllRoot = process.env.XXMI_DLL_SRC;
const baselineRoot = process.env.MIGOTO_DLL_SRC;
describe.skipIf(!dllRoot || !baselineRoot)('real DLL (docs/09-roadmap.md M1 acceptance)', () => {
  let spec: Spec | undefined;
  const get = (): Spec =>
    (spec ??= extractSpec({ dllRoot: dllRoot ?? '', baselineRoot: baselineRoot ?? '' }).spec);

  it('covers every section prefix and operator token with a source', () => {
    const s = get();
    expect(s.sections.length).toBeGreaterThanOrEqual(25);
    expect(s.sections.every((x) => x.source.startsWith('IniHandler.cpp:'))).toBe(true);
    expect(s.operators.tokens.every((t) => t.source === 'CommandList.cpp:operator_tokens')).toBe(
      true,
    );
    for (const token of ['===', '<<', '>>', '**', '//', '&&', '||', '&', '|', '^', '~']) {
      expect(s.operators.tokens.map((t) => t.token)).toContain(token);
    }
  });

  it('flags XXMI features', () => {
    const s = get();
    expect(flag(s.commands, 'store')).toBe(true);
    expect(flag(s.sections, 'Pool')).toBe(true);
    expect(s.resourceMembers.length).toBeGreaterThan(0);
    expect(s.resourceMembers.every((m) => m.xxmi)).toBe(true);
    // Vanilla 3DMigoto already has these; they must not be flagged.
    for (const name of [
      'run',
      'if',
      'endif',
      'global',
      'persist',
      'local',
      'handling',
      'drawindexed',
    ]) {
      expect(flag(s.commands, name), name).toBe(false);
    }
    const resource = s.sections.find((x) => x.name === 'Resource');
    for (const key of ['type', 'format', 'filename', 'stride']) {
      expect(flag(resource?.keys ?? [], key), key).toBe(false);
    }
  });

  it('is byte-identical across runs', () => {
    const again = extractSpec({ dllRoot: dllRoot ?? '', baselineRoot: baselineRoot ?? '' }).spec;
    expect(serializeSpec(again)).toEqual(serializeSpec(get()));
  });
});
