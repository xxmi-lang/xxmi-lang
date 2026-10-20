import { describe, expect, it } from 'vitest';
import {
  defaultSpec,
  docAbove,
  commentBlocks,
  findImplementation,
  listSpec,
  lookupSpec,
  type SourceTree,
} from '../src/index.ts';

const spec = defaultSpec();

describe('lookupSpec', () => {
  it('finds a command with its XXMI flag and DLL source', () => {
    const [store] = lookupSpec(spec, { name: 'store' });

    expect(store).toMatchObject({ kind: 'command', name: 'store', xxmi: true });
    expect(store?.source).toMatch(/^CommandList\.cpp:/);
  });

  it('matches section prefixes, bracketed names and ->members', () => {
    expect(lookupSpec(spec, { name: '[ResourceFoo]', kind: 'section' })[0]?.name).toBe('Resource');
    expect(lookupSpec(spec, { name: '->Region' })[0]).toMatchObject({
      kind: 'member',
      name: '->region',
    });
  });

  it('narrows keys by section and returns nothing for unknown names', () => {
    const inResource = lookupSpec(spec, { name: 'type', kind: 'key', section: 'ResourceX' });

    expect(inResource.map((e) => e.section)).toEqual(['Resource']);
    expect(lookupSpec(spec, { name: 'type', kind: 'key', section: 'NoSuchSection' })).toEqual([]);
    expect(lookupSpec(spec, { name: 'definitely-not-a-thing' })).toEqual([]);
  });

  it('reports operator precedence', () => {
    const plus = lookupSpec(spec, { name: '+', kind: 'operator' })[0];

    expect(plus?.facts.join()).toMatch(/precedence level \d+/);
  });
});

describe('listSpec', () => {
  it('lists keys of a section and requires the section', () => {
    expect(listSpec(spec, 'key', 'Resource').map((i) => i.name)).toContain('filename');
    expect(() => listSpec(spec, 'key')).toThrow(/needs a known section/);
  });
});

describe('doc comments', () => {
  it('reads the plain block above a header and the structured tags', () => {
    const lines = [
      '; plain',
      ';   indented',
      '',
      ';; @desc Does a thing.',
      ';; @in   Resource\\X\\A   first (required)',
      ';; @out  $\\X\\done',
      ';; @since X 1.0',
      ';; @example',
      ';;   run = CommandList\\X\\Thing',
      '[CommandListThing]',
    ];

    const doc = docAbove(lines, 9);

    expect(doc?.line).toBe(4);
    expect(doc?.text).toBe('Does a thing.');
    expect(doc?.tags?.in).toEqual([{ name: 'Resource\\X\\A', description: 'first (required)' }]);
    expect(doc?.tags?.out[0]?.name).toBe('$\\X\\done');
    expect(doc?.tags?.since).toBe('X 1.0');
    expect(doc?.tags?.example).toBe('  run = CommandList\\X\\Thing');
    expect(commentBlocks(lines).map((b) => b.line)).toEqual([1, 4]);
  });

  it('returns nothing when the line above is not a comment', () => {
    expect(docAbove(['x = 1', '', '[A]'], 2)).toBeUndefined();
  });
});

describe('findImplementation', () => {
  const files: Record<string, string> = {
    'DirectX11/CommandList.cpp': [
      'static bool ParseStoreCommand(int a)',
      '{',
      '\tReader r(L"store", a);',
      '}',
      'bool ParseCommandListGeneralCommands(int a)',
      '{',
      '\tif (!wcscmp(key, L"store")) {',
      '\t}',
      '}',
    ].join('\n'),
    'DirectX11/Other.cpp': 'void Log() { puts("store"); }',
    'DirectX11/notes.txt': 'store',
  };
  const tree: SourceTree = {
    files: () => Object.keys(files),
    read: (p) => files[p],
  };

  it('ranks the spec symbol definition first, then literals with their function', () => {
    const result = findImplementation(spec, 'store', tree);

    expect(result.hits.map((h) => `${h.path}:${h.line}:${h.role}`)).toEqual([
      'DirectX11/CommandList.cpp:5:definition',
      'DirectX11/CommandList.cpp:3:literal',
      'DirectX11/CommandList.cpp:7:literal',
      'DirectX11/Other.cpp:1:mention',
    ]);
    expect(result.hits[1]?.inFunction).toBe('ParseStoreCommand');
  });

  it('falls back to a word search for features outside the spec', () => {
    const result = findImplementation(spec, 'Reader', tree);

    expect(result.entries).toEqual([]);
    expect(result.hits[0]).toMatchObject({
      path: 'DirectX11/CommandList.cpp',
      line: 3,
      role: 'mention',
    });
  });
});
