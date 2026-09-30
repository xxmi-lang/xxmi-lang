import type { SpecEnum, SpecOperators, SpecPrecedenceLevel } from '@xxmi-lang/core';
import {
  AnchorNotFoundError,
  findCalls,
  splitTopLevel,
  stringLiteral,
  stringLiterals,
  type CppFile,
} from './cpp.ts';
import type { Extraction } from './dll.ts';

const CL = 'CommandList.cpp';

/** `DEFINE_OPERATOR(name, "pattern", fn)` → name ↦ pattern. */
export function operatorPatterns(file: CppFile): Map<string, string> {
  const patterns = new Map<string, string>();
  for (const call of findCalls(file.code, 'DEFINE_OPERATOR')) {
    const [name, pattern] = call.args;
    const value = stringLiteral(pattern ?? '');
    if (name && value !== undefined && !patterns.has(name)) patterns.set(name, value);
  }
  if (patterns.size === 0)
    throw new AnchorNotFoundError(file.name, 'DEFINE_OPERATOR(…) definitions');
  return patterns;
}

/**
 * Precedence from the order of `transform_operators_recursive(&tree, <group>, …, right, unary)`
 * calls: the expression parser folds each group in turn, tightest first.
 */
function precedence(file: CppFile, patterns: Map<string, string>): SpecPrecedenceLevel[] {
  const levels: SpecPrecedenceLevel[] = [];
  for (const call of findCalls(file.code, 'transform_operators_recursive')) {
    if (call.args[0] !== '&tree') continue;
    const [, group = '', , right, unary] = call.args;
    if (levels.some((l) => l.group === group)) continue;
    const operators = splitTopLevel(file.arrayInitializer(group))
      .map((ref) => patterns.get(ref.replace(/^&/, '')))
      .filter((p): p is string => p !== undefined && !/^[a-z_]\w*$/i.test(p));
    levels.push({
      level: levels.length,
      group,
      associativity: right === 'true' ? 'right' : 'left',
      unary: unary === 'true',
      operators,
      source: `${file.name}:${group}`,
      xxmi: false,
    });
  }
  if (levels.length === 0) {
    throw new AnchorNotFoundError(file.name, 'transform_operators_recursive(&tree, …) calls');
  }
  return levels;
}

export function extractOperators(x: Extraction): {
  operators: SpecOperators;
  patterns: Map<string, string>;
} {
  const file = x.dll.file(CL);
  const tokens = stringLiterals(file.arrayInitializer('operator_tokens')).map((token) => ({
    token,
    source: `${CL}:operator_tokens`,
    xxmi: false,
  }));
  if (tokens.length === 0) throw new AnchorNotFoundError(CL, 'operator_tokens', 'empty');
  const patterns = operatorPatterns(file);
  return { operators: { tokens, precedence: precedence(file, patterns) }, patterns };
}

/** Every `EnumName_t<…> XNames[] = { {L"name", …}, … }` table in the DLL. */
export function extractEnums(x: Extraction): SpecEnum[] {
  const enums: SpecEnum[] = [];
  for (const file of x.dll.all()) {
    for (const name of file.enumTableNames()) {
      if (enums.some((e) => e.name === name)) continue;
      const source = `${file.name}:${name}`;
      const values = splitTopLevel(file.arrayInitializer(name))
        .map((entry) => stringLiteral(splitTopLevel(entry.replace(/^\{|\}$/g, ''))[0] ?? ''))
        .filter((v): v is string => v !== undefined)
        .map((v) => ({ name: v, source, xxmi: false }));
      enums.push({ name, values, source, xxmi: false });
    }
  }
  if (enums.length === 0 && x.strict)
    throw new AnchorNotFoundError('DirectX11', 'EnumName_t tables');
  // `format =` values: ParseFormatString (util.h) accepts these names, with or without the
  // DXGI_FORMAT_ prefix, or the numeric DXGI_FORMAT value (the index in this table).
  const formats = x.step(undefined, () => {
    const file = x.dll.file('util.h');
    const names = stringLiterals(file.arrayInitializer('DXGIFormats'));
    if (names.length === 0) throw new AnchorNotFoundError(file.name, 'DXGIFormats', 'empty');
    const source = `${file.name}:DXGIFormats`;
    return {
      name: 'DXGIFormats',
      values: names.map((name) => ({ name, source, xxmi: false })),
      source,
      xxmi: false,
    };
  });
  if (formats) enums.push(formats);
  return enums;
}
