import { describe, expect, it } from 'vitest';
import {
  CppFile,
  findCalls,
  ifBlocks,
  splitTopLevel,
  stringLiteral,
  stripComments,
} from '../src/cpp.ts';

describe('stripComments', () => {
  it('keeps comment markers inside string literals', () => {
    const code = 'x = L"//"; // gone\ny = "/*"; /* gone\n too */ z = 1;';
    const stripped = stripComments(code);
    expect(stripped).toContain('L"//"');
    expect(stripped).toContain('"/*"');
    expect(stripped).not.toContain('gone');
    expect(stripped).toContain('z = 1');
    expect(stripped.length).toBe(code.length);
    expect(stripped.split('\n')).toHaveLength(code.split('\n').length);
  });

  it('handles escaped quotes and char literals', () => {
    expect(stripComments(String.raw`a = "\"//"; b = '"'; // c`)).toBe(
      String.raw`a = "\"//"; b = '"';     `,
    );
  });
});

describe('CppFile', () => {
  const file = new CppFile(
    'x.cpp',
    `static int helper(int a);
static bool Foo(const wchar_t *s,
		int n)
{
	if (helper(1)) { return true; }
	return false;
}
void Bar::Baz() const
{
	call();
}
static int Table[] = { 1, 2, /* 3, */ 4 };
#define LIST \\
	L"a", \\
	L"b"
#define ONE L"x"`,
  );

  it('finds a definition, not a declaration or call', () => {
    expect(file.functionBody('Foo')).toContain('return false');
    // `helper` is only declared and called, never defined.
    expect(file.tryFunctionBody('helper')).toBeUndefined();
  });

  it('finds qualified member functions with trailing qualifiers', () => {
    expect(file.functionBody('Bar::Baz').trim()).toBe('call();');
  });

  it('throws a named error for a missing anchor', () => {
    expect(() => file.functionBody('Missing')).toThrow(/x\.cpp: could not find function Missing/);
  });

  it('reads array initializers without comments', () => {
    expect(splitTopLevel(file.arrayInitializer('Table'))).toEqual(['1', '2', '4']);
  });

  it('reads multi-line macros', () => {
    expect(file.macro('LIST')?.match(/L"\w"/g)).toEqual(['L"a"', 'L"b"']);
    expect(file.macro('ONE')?.trim()).toBe('L"x"');
    expect(file.macro('NOPE')).toBeUndefined();
  });
});

describe('helpers', () => {
  it('splits arguments at top-level commas only', () => {
    expect(splitTopLevel('a, f(b, c), L"d,e", {1, 2}')).toEqual([
      'a',
      'f(b, c)',
      'L"d,e"',
      '{1, 2}',
    ]);
  });

  it('reads wide and narrow string literals', () => {
    expect(stringLiteral('L"key"')).toBe('key');
    expect(stringLiteral('"a\\"b"')).toBe('a"b');
    expect(stringLiteral('key')).toBeUndefined();
  });

  it('finds calls but not definitions of other names', () => {
    const calls = findCalls(
      'GetIniInt(s, L"a", 0); MyGetIniInt(s, L"b"); x.GetIniInt(s, L"c");',
      'GetIniInt',
    );
    expect(calls.map((c) => c.args[1])).toEqual(['L"a"']);
  });

  it('splits a flat if-chain', () => {
    const blocks = ifBlocks(`
      if (!wcscmp(key, L"a")) return 1;
      if (!wcscmp(key, L"b")) { if (x) y(); return 2; }
      else if (!wcscmp(key, L"c")) z();`);
    expect(blocks.map((b) => b.condition)).toEqual([
      '!wcscmp(key, L"a")',
      '!wcscmp(key, L"b")',
      '!wcscmp(key, L"c")',
    ]);
  });
});
