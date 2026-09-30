import type { SpecBuiltinSection, SpecDynamicKeys, SpecKey, SpecSection } from '@xxmi-lang/core';
import {
  AnchorNotFoundError,
  findCalls,
  ifBlocks,
  stringLiteral,
  stringLiterals,
  type CppFile,
} from './cpp.ts';
import type { Extraction } from './dll.ts';

const INI = 'IniHandler.cpp';

/** Where a section's keys are read, besides literal-section reads found by scanning. */
interface KeySource {
  file: string;
  /**
   * Function whose `GetIni*(section, L"key", …)` calls read keys of this section. A list names
   * alternatives across DLL versions: every one that exists is read, and it's an error only if
   * none does.
   */
  fn?: string | string[];
  /** `wchar_t *X[]` list of keys a command-list section parses outside the command list. */
  whitelist?: string;
  /** Function that compares keys by hand: `wcscmp(key->c_str(), L"include")`. */
  compares?: string;
}

/**
 * The map from section name (as in the DLL's section tables) to the code that reads its keys.
 * This is the one piece of hand-kept knowledge in the extractor: when the DLL moves a parser,
 * extraction fails with the missing anchor named, and this map gets updated.
 */
export const SECTION_KEY_SOURCES: Record<string, KeySource[]> = {
  TextureOverride: [
    { file: INI, whitelist: 'TextureOverrideIniKeys' },
    { file: INI, whitelist: 'TextureOverrideFuzzyMatchesIniKeys' },
    { file: INI, fn: 'ParseTextureOverrideSections' },
    { file: INI, fn: 'parse_texture_override_common' },
    { file: INI, fn: 'parse_texture_override_fuzzy_match' },
  ],
  ShaderOverride: [
    { file: INI, whitelist: 'ShaderOverrideIniKeys' },
    { file: INI, fn: 'ParseShaderOverrideSections' },
  ],
  ShaderRegex: [
    { file: INI, whitelist: 'ShaderRegexIniKeys' },
    { file: INI, fn: 'parse_shader_regex_section_main' },
  ],
  CustomShader: [
    { file: INI, whitelist: 'CustomShaderIniKeys' },
    { file: INI, fn: 'ParseCustomShaderSections' },
    { file: INI, fn: 'ParseBlendState' },
    { file: INI, fn: 'ParseDepthStencilState' },
    { file: INI, fn: 'ParseRSState' },
    { file: INI, fn: 'ParseTopology' },
    { file: INI, fn: 'ParseSamplerState' },
  ],
  // XXMI split the per-section parser (ParseResourceSection) out of the loop that vanilla
  // 3DMigoto still reads keys in (ParseResourceSections).
  Resource: [
    { file: INI, fn: ['ParseResourceSection', 'ParseResourceSections'] },
    { file: INI, fn: 'ParseResourceInitialData' },
  ],
  // A pool's resource template is parsed with ParseResourceSection(section_name, L"template").
  Pool: [
    { file: INI, fn: 'ParseResourcePoolSection' },
    { file: INI, fn: 'ParseResourceSection' },
    { file: INI, fn: 'ParseResourceInitialData' },
  ],
  Key: [
    { file: INI, fn: 'RegisterPresetKeyBindings' },
    { file: 'Override.cpp', fn: 'Override::ParseIniSection' },
    { file: 'Override.cpp', fn: 'KeyOverrideCycle::ParseIniSection' },
  ],
  Preset: [
    { file: INI, fn: 'ParsePresetOverrideSections' },
    { file: 'Override.cpp', fn: 'Override::ParseIniSection' },
  ],
  Include: [{ file: INI, compares: 'ParseIncludedIniFiles' }],
};

/** Functions that read ini values, and the value type each implies. */
const GETTERS: Record<string, { type: string; repeatable?: boolean }> = {
  GetIniInt: { type: 'int' },
  GetIniFloat: { type: 'float' },
  GetIniBool: { type: 'bool' },
  GetIniString: { type: 'string' },
  GetIniStringAndLog: { type: 'string' },
  GetIniWstring: { type: 'string' },
  GetIniHexString: { type: 'hex' },
  GetIniBoolOrInt: { type: 'bool-or-int' },
  GetIniBoolIntOrEnum: { type: 'bool-or-int' },
  GetIniHash: { type: 'hash' },
  GetIniEnum: { type: 'enum' },
  GetIniEnumClass: { type: 'enum' },
  GetIniStringMultipleKeys: { type: 'string', repeatable: true },
  GetPrivateProfileInt: { type: 'int' },
  GetPrivateProfileString: { type: 'string' },
  RegisterIniKeyBinding: { type: 'key-binding' },
  IniHasKey: { type: 'unknown' },
};

interface KeyRead {
  section: string;
  key: SpecKey;
}

/** Every getter call in `code` whose key argument is a literal. */
function keyReads(code: string, file: string, symbol: string): KeyRead[] {
  const reads: { index: number; read: KeyRead }[] = [];
  for (const [getter, info] of Object.entries(GETTERS)) {
    for (const call of findCalls(code, getter)) {
      const key = stringLiteral(call.args[1] ?? '');
      if (key === undefined) continue;
      let valueType = info.type;
      if (valueType === 'enum' || getter === 'GetIniBoolIntOrEnum') {
        const table = call.args.find((a) => /^\w+Names$/.test(a));
        valueType = table
          ? `${getter === 'GetIniBoolIntOrEnum' ? 'bool-or-int|' : ''}enum:${table}`
          : valueType;
      }
      // A string handed straight to ParseFormatString is a DXGI format name (util.h DXGIFormats).
      const after = code.slice(call.index, call.index + 400);
      const nextRead = after.slice(1).search(/\bGetIni\w*\s*\(/);
      const window = nextRead < 0 ? after : after.slice(0, nextRead + 1);
      if (valueType === 'string' && /\bParseFormatString\s*\(/.test(window)) {
        valueType = 'enum:DXGIFormats';
      }
      const spec: SpecKey = { name: key, valueType, source: `${file}:${symbol}`, xxmi: false };
      if (info.repeatable) spec.repeatable = true;
      reads.push({ index: call.index, read: { section: call.args[0] ?? '', key: spec } });
    }
  }
  return reads.sort((a, b) => a.index - b.index).map((r) => r.read);
}

interface SectionEntry {
  name: string;
  prefix: boolean;
}

function sectionTable(file: CppFile, name: string): SectionEntry[] {
  const body = file.arrayInitializer(name);
  const entries = [...body.matchAll(/\{\s*L"([^"]+)"\s*,\s*(true|false)\s*\}/g)].map((m) => ({
    name: m[1] ?? '',
    prefix: m[2] === 'true',
  }));
  if (entries.length === 0)
    throw new AnchorNotFoundError(file.name, name, 'no {L"…", bool} entries');
  return entries;
}

/** Keys of a `wchar_t *X[] = { L"a", MACRO, NULL }` list, expanding same-file macros. */
function whitelist(file: CppFile, name: string): string[] {
  const keys: string[] = [];
  for (const item of file
    .arrayInitializer(name)
    .split(',')
    .map((s) => s.trim())) {
    const literal = stringLiteral(item);
    if (literal !== undefined) keys.push(literal);
    else if (/^[A-Z_][A-Z0-9_]*$/.test(item) && item !== 'NULL') {
      const macro = file.macro(item);
      if (macro === undefined) throw new AnchorNotFoundError(file.name, `#define ${item}`);
      keys.push(...stringLiterals(macro));
    }
  }
  return keys;
}

/** `whitelisted_duplicate_key`: sections whose keys may all repeat, and per-section keys. */
function duplicateWhitelist(file: CppFile): {
  sections: SectionEntry[];
  keys: Map<string, string[]>;
} {
  const sections: SectionEntry[] = [];
  const keys = new Map<string, string[]>();
  for (const block of ifBlocks(file.functionBody('whitelisted_duplicate_key'))) {
    const m = /_wcsn?icmp\s*\(\s*section\s*,\s*L"([^"]+)"/.exec(block.condition);
    if (!m?.[1]) continue;
    const section = m[1].toLowerCase();
    const prefix = block.condition.includes('_wcsnicmp');
    const keyNames = [...block.body.matchAll(/_wcsicmp\s*\(\s*key\s*,\s*L"([^"]+)"/g)].map(
      (k) => k[1] ?? '',
    );
    if (keyNames.length > 0) keys.set(section, keyNames);
    else sections.push({ name: section, prefix });
  }
  return { sections, keys };
}

function matchesSection(section: string, entry: SectionEntry): boolean {
  const s = section.toLowerCase();
  const e = entry.name.toLowerCase();
  return entry.prefix ? s.startsWith(e) : s === e;
}

function addKey(keys: SpecKey[], key: SpecKey): void {
  const existing = keys.find((k) => k.name.toLowerCase() === key.name.toLowerCase());
  if (!existing) {
    keys.push(key);
    return;
  }
  if (existing.valueType === 'unknown' && key.valueType !== 'unknown') {
    existing.valueType = key.valueType;
    existing.source = key.source;
  }
  if (key.repeatable) existing.repeatable = true;
}

export function extractSections(x: Extraction): SpecSection[] {
  const ini = x.dll.file(INI);
  const commandList = sectionTable(ini, 'CommandListSections');
  const regular = sectionTable(ini, 'RegularSections');
  const bareLines = x.step([], () => sectionTable(ini, 'AllowLinesWithoutEquals'));
  const bareLinesForCommandLists = x.step(false, () =>
    ini.functionBody('DoesSectionAllowLinesWithoutEquals').includes('IsCommandListSection('),
  );
  const noDuplicateWarningsForCommandLists = x.step(false, () =>
    /IsCommandListSection\s*\(/.test(ini.functionBody('ParseIniSectionLine')),
  );
  const duplicates = x.step({ sections: [], keys: new Map<string, string[]>() }, () =>
    duplicateWhitelist(ini),
  );

  // Reads with a literal section name (`GetIniBool(L"Logging", L"calls", …)`), in any file.
  const literalReads: KeyRead[] = [];
  for (const file of x.dll.all()) {
    if (!file.name.endsWith('.cpp')) continue;
    for (const read of keyReads(file.code, file.name, 'literal')) {
      const section = stringLiteral(read.section);
      if (section === undefined) continue;
      literalReads.push({ section, key: read.key });
    }
  }

  const sections: SpecSection[] = [];
  const tables: [SectionEntry[], 'commandlist' | 'regular', string][] = [
    [commandList, 'commandlist', 'CommandListSections'],
    [regular, 'regular', 'RegularSections'],
  ];
  for (const [table, kind, tableName] of tables) {
    for (const entry of table) {
      const isCommandList = kind === 'commandlist';
      const keys: SpecKey[] = [];
      const dynamicKeys: SpecDynamicKeys[] = [];
      const whitelisted = new Set<string>();

      for (const source of SECTION_KEY_SOURCES[entry.name] ?? []) {
        const file = x.step(undefined, () => x.dll.file(source.file));
        if (!file) continue;
        if (source.whitelist) {
          const names = x.step([], () => whitelist(file, source.whitelist ?? ''));
          for (const name of names) {
            whitelisted.add(name.toLowerCase());
            addKey(keys, {
              name,
              valueType: 'unknown',
              source: `${file.name}:${source.whitelist}`,
              xxmi: false,
            });
          }
        }
        for (const [fn, body] of functionBodies(x, file, source.fn)) {
          for (const read of keyReads(body, file.name, fn)) {
            if (stringLiteral(read.section) !== undefined) continue; // literal reads handled below
            // Command-list sections only own their whitelisted keys; anything else is a command.
            if (isCommandList && !whitelisted.has(read.key.name.toLowerCase())) continue;
            addKey(keys, read.key);
          }
          if (/\bParseIniParamName\s*\(/.test(body)) {
            addDynamic(dynamicKeys, {
              kind: 'ini_param',
              source: `${file.name}:${fn}`,
              xxmi: false,
            });
          }
          if (/\[0\]\s*==\s*L'\$'/.test(body)) {
            addDynamic(dynamicKeys, {
              kind: 'variable',
              source: `${file.name}:${fn}`,
              xxmi: false,
            });
          }
        }
        if (source.compares) {
          const fn = source.compares;
          const body = x.step(undefined, () => file.functionBody(fn));
          if (body === undefined) continue;
          for (const m of body.matchAll(/wcs(?:i)?cmp\s*\(\s*key->c_str\(\)\s*,\s*L"([^"]+)"/g)) {
            addKey(keys, {
              name: m[1] ?? '',
              valueType: 'string',
              source: `${file.name}:${fn}`,
              xxmi: false,
            });
          }
        }
      }

      for (const read of literalReads) {
        if (matchesSection(read.section, entry)) addKey(keys, read.key);
      }

      const lower = entry.name.toLowerCase();
      for (const name of duplicates.keys.get(lower) ?? []) {
        const key = keys.find((k) => k.name.toLowerCase() === name.toLowerCase());
        if (key) key.repeatable = true;
      }

      sections.push({
        name: entry.name,
        prefix: entry.prefix,
        kind,
        allowsBareLines:
          (isCommandList && bareLinesForCommandLists) ||
          bareLines.some((b) => b.name.toLowerCase() === lower),
        allowsDuplicateKeys:
          (isCommandList && noDuplicateWarningsForCommandLists) ||
          duplicates.sections.some((d) => d.name === lower),
        keys,
        dynamicKeys,
        source: `${INI}:${tableName}`,
        xxmi: false,
      });
    }
  }
  return sections;
}

/** Bodies of the named function(s) that exist; an anchor error if none of them does. */
function functionBodies(
  x: Extraction,
  file: CppFile,
  fn: string | string[] | undefined,
): [string, string][] {
  if (fn === undefined) return [];
  const names = Array.isArray(fn) ? fn : [fn];
  return x.step([], () => {
    const found = names.flatMap((name): [string, string][] => {
      const body = file.tryFunctionBody(name);
      return body === undefined ? [] : [[name, body]];
    });
    if (found.length === 0)
      throw new AnchorNotFoundError(file.name, `function ${names.join(' or ')}`);
    return found;
  });
}

function addDynamic(list: SpecDynamicKeys[], entry: SpecDynamicKeys): void {
  if (!list.some((d) => d.kind === entry.kind)) list.push(entry);
}

/** Sections the DLL parses from its own ini excerpt in `InsertBuiltInIniSections`. */
export function extractBuiltinSections(x: Extraction): SpecBuiltinSection[] {
  return x.step([], () => {
    const fn = 'InsertBuiltInIniSections';
    const source = `${INI}:${fn}`;
    const names = stringLiterals(x.dll.file(INI).functionBody(fn))
      .map((line) => /^\[([^\]]+)\]/.exec(line.trim())?.[1])
      .filter((name): name is string => name !== undefined);
    if (names.length === 0) throw new AnchorNotFoundError(INI, fn, 'no [section] lines');
    return names.map((name) => ({ name, source, xxmi: false }));
  });
}
