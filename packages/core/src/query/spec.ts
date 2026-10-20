/**
 * Read-only lookups over the merged spec (`xxmi query`, MCP `spec_lookup` / `spec_list`).
 * Matching follows `SpecLookup`, so `ResourceFoo` finds the `Resource` section and a prefix
 * command like `commandlistFoo` finds `commandlist`.
 */
import { SpecLookup } from '../spec/lookup.ts';
import type { Documented, Sourced, Spec, SpecSection } from '../spec/types.ts';

export const SPEC_ENTRY_KINDS = [
  'section',
  'key',
  'command',
  'operator',
  'function',
  'member',
  'enum',
] as const;
export type SpecEntryKind = (typeof SPEC_ENTRY_KINDS)[number];

export interface SpecEntry extends Documented {
  kind: SpecEntryKind;
  name: string;
  /** For keys: the spec section that reads the key. */
  section?: string;
  /** In the XXMI fork but not in vanilla 3DMigoto. */
  xxmi: boolean;
  /** `<file>:<symbol>` in the XXMI DLL source, or `manual`. */
  source: string;
  /** One line of facts the DLL gives (value type, argument count, precedence, …). */
  facts: string[];
  /** Keyword values (commands) or enum values. */
  values?: string[];
}

export interface SpecQuery {
  name: string;
  kind?: SpecEntryKind;
  /** Restricts `key` lookups to the section named (or whose prefix matches) this. */
  section?: string;
}

/** `[Name]`, `->Name` and surrounding space removed. */
function normalize(name: string): string {
  return name
    .trim()
    .replace(/^\[(.*)\]$/, '$1')
    .replace(/^->/, '');
}

function base(
  entry: Sourced & Documented,
): Pick<SpecEntry, 'xxmi' | 'source' | 'doc'> & Documented {
  return {
    xxmi: entry.xxmi,
    source: entry.source,
    ...(entry.doc ? { doc: entry.doc } : {}),
    ...(entry.example ? { example: entry.example } : {}),
    ...(entry.since ? { since: entry.since } : {}),
    ...(entry.deprecated ? { deprecated: entry.deprecated } : {}),
    ...(entry.note ? { note: entry.note } : {}),
  };
}

function sectionEntry(s: SpecSection): SpecEntry {
  return {
    kind: 'section',
    name: s.name,
    ...base(s),
    facts: [
      s.prefix ? 'prefix match (any section name starting with it)' : 'exact name',
      s.kind === 'commandlist' ? 'command-list section' : 'regular section',
      `${s.keys.length} keys`,
    ],
  };
}

function lookupSections(spec: Spec, lookup: SpecLookup, name: string): SpecEntry[] {
  const lower = name.toLowerCase();
  const exact = spec.sections.find((s) => s.name.toLowerCase() === lower);
  const found = exact ?? lookup.sectionKind(name);
  return found ? [sectionEntry(found)] : [];
}

function lookupKeys(spec: Spec, lookup: SpecLookup, q: SpecQuery, name: string): SpecEntry[] {
  const only = q.section ? lookup.sectionKind(normalize(q.section)) : undefined;
  if (q.section && !only) return [];
  const out: SpecEntry[] = [];
  for (const s of only ? [only] : spec.sections) {
    const key = lookup.key(s, name);
    if (!key) continue;
    out.push({
      kind: 'key',
      name: key.name,
      section: s.name,
      ...base(key),
      facts: [
        `type ${key.valueType}`,
        ...(key.repeatable ? ['repeatable'] : []),
        `read in [${s.name}${s.prefix ? '…' : ''}]`,
      ],
    });
  }
  return out;
}

function lookupCommands(spec: Spec, lookup: SpecLookup, name: string): SpecEntry[] {
  const lower = name.toLowerCase();
  const matches = spec.commands.filter((c) => c.name.toLowerCase() === lower);
  const viaPrefix = lookup.command(name);
  if (viaPrefix && !matches.includes(viaPrefix)) matches.push(viaPrefix);
  return matches.map((c) => ({
    kind: 'command' as const,
    name: c.name,
    ...base(c),
    facts: [
      `${c.kind} command`,
      c.match === 'prefix' ? 'matches by prefix' : 'exact name',
      ...(c.argCount === undefined ? [] : [`${c.argCount} comma-separated arguments`]),
      ...(c.indirect ? ['first argument is an indirect-arguments resource'] : []),
    ],
    ...(c.values.length > 0 ? { values: c.values.map((v) => v.name) } : {}),
  }));
}

function lookupOperators(spec: Spec, name: string): SpecEntry[] {
  return spec.operators.tokens
    .filter((t) => t.token === name)
    .map((t) => {
      const level = spec.operators.precedence.find((p) => p.operators.includes(t.token));
      return {
        kind: 'operator' as const,
        name: t.token,
        ...base(t),
        facts: level
          ? [
              `precedence level ${level.level} (0 binds tightest)`,
              `${level.associativity}-associative`,
              level.unary ? 'unary' : 'binary',
            ]
          : [],
      };
    });
}

function lookupNamed(spec: Spec, kind: 'function' | 'member' | 'enum', name: string): SpecEntry[] {
  const lower = name.toLowerCase();
  if (kind === 'function') {
    return spec.functions
      .filter((f) => f.name.toLowerCase() === lower)
      .map((f) => ({ kind, name: f.name, ...base(f), facts: ['built-in math function'] }));
  }
  if (kind === 'member') {
    return spec.resourceMembers
      .filter((m) => m.name.toLowerCase() === lower)
      .map((m) => ({
        kind,
        name: `->${m.name}`,
        ...base(m),
        facts: [`arguments: ${m.args.length > 0 ? m.args.join(', ') : 'none'}`],
      }));
  }
  return spec.enums
    .filter((e) => e.name.toLowerCase() === lower)
    .map((e) => ({
      kind,
      name: e.name,
      ...base(e),
      facts: ['enum table'],
      values: e.values.map((v) => v.name),
    }));
}

/** All spec entries named `q.name`, of `q.kind` or of any kind. */
export function lookupSpec(spec: Spec, q: SpecQuery, lookup = new SpecLookup(spec)): SpecEntry[] {
  const name = normalize(q.name);
  if (name === '') return [];
  const kinds = q.kind ? [q.kind] : SPEC_ENTRY_KINDS;
  return kinds.flatMap((kind): SpecEntry[] => {
    switch (kind) {
      case 'section':
        return lookupSections(spec, lookup, name);
      case 'key':
        return lookupKeys(spec, lookup, q, name);
      case 'command':
        return lookupCommands(spec, lookup, name);
      case 'operator':
        return lookupOperators(spec, name);
      default:
        return lookupNamed(spec, kind, name);
    }
  });
}

export interface SpecListItem {
  name: string;
  /** First line of the doc, or empty when the entry has no overlay doc. */
  summary: string;
  xxmi: boolean;
}

function summarize(doc: string | undefined): string {
  return doc?.split(/\r?\n/)[0]?.trim() ?? '';
}

/**
 * Names (with one-line docs) of one kind, for discovery. `key` needs `section`: the keys that
 * section reads.
 */
export function listSpec(
  spec: Spec,
  kind: SpecEntryKind,
  section?: string,
  lookup = new SpecLookup(spec),
): SpecListItem[] {
  const item = (e: { name: string; doc?: string | undefined; xxmi: boolean }): SpecListItem => ({
    name: e.name,
    summary: summarize(e.doc),
    xxmi: e.xxmi,
  });
  switch (kind) {
    case 'section':
      return spec.sections.map(item);
    case 'key': {
      const found = section ? lookup.sectionKind(normalize(section)) : undefined;
      if (!found) throw new Error('listing keys needs a known section, e.g. "Resource"');
      return found.keys.map(item);
    }
    case 'command':
      return spec.commands.map(item);
    case 'operator':
      return spec.operators.tokens.map((t) => item({ name: t.token, doc: t.doc, xxmi: t.xxmi }));
    case 'function':
      return spec.functions.map(item);
    case 'member':
      return spec.resourceMembers.map((m) => item({ ...m, name: `->${m.name}` }));
    case 'enum':
      return spec.enums.map(item);
  }
}
