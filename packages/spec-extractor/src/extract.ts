import type { Spec, SpecMeta } from '@xxmi-lang/core';
import { functions, extractCommands } from './commands.ts';
import { AnchorNotFoundError } from './cpp.ts';
import { DllSource, Extraction, gitInfo, type GitInfo } from './dll.ts';
import { extractEnums, extractOperators } from './operators.ts';
import { extractBuiltinSections, extractSections } from './sections.ts';

/** Bump when the output shape or extraction rules change, so regenerated specs are expected. */
export const EXTRACTOR_VERSION = 3;

export type SpecBody = Omit<Spec, 'meta'>;

/** Extracts one DLL. `strict` fails on any missing anchor (use for the XXMI DLL). */
export function extractBody(
  dll: DllSource,
  strict: boolean,
): { body: SpecBody; warnings: string[] } {
  const x = new Extraction(dll, strict);
  // A partial checkout (e.g. a CI sparse checkout) would otherwise silently drop keys.
  for (const component of dll.missingComponents) {
    x.step(undefined, () => {
      throw new AnchorNotFoundError(
        `${component}/`,
        'directory',
        'check out the whole repo or add it to the sparse checkout',
      );
    });
  }
  const { operators, patterns } = extractOperators(x);
  const { commands, resourceMembers } = extractCommands(x);
  const body: SpecBody = {
    sections: extractSections(x),
    builtinSections: extractBuiltinSections(x),
    commands,
    resourceMembers,
    functions: x.step([], () => functions(dll.file('CommandList.cpp'), patterns)),
    operators,
    enums: extractEnums(x),
  };
  return { body, warnings: x.warnings };
}

/** Identity of every flaggable entry, case-insensitive like the DLL. */
function* identities(body: SpecBody): Generator<[string, { xxmi: boolean }]> {
  const id = (...parts: string[]): string => parts.join('\u0000').toLowerCase();
  for (const b of body.builtinSections) yield [id('builtin', b.name), b];
  for (const s of body.sections) {
    yield [id('section', s.name), s];
    for (const k of s.keys) yield [id('key', s.name, k.name), k];
    for (const d of s.dynamicKeys) yield [id('dynamic', s.name, d.kind), d];
  }
  for (const c of body.commands) {
    yield [id('command', c.kind, c.name), c];
    for (const v of c.values) yield [id('value', c.kind, c.name, v.name), v];
  }
  for (const m of body.resourceMembers) yield [id('member', m.name), m];
  for (const f of body.functions) yield [id('function', f.name), f];
  for (const t of body.operators.tokens) yield [id('token', t.token), t];
  for (const p of body.operators.precedence) yield [id('precedence', p.group), p];
  for (const e of body.enums) {
    yield [id('enum', e.name), e];
    for (const v of e.values) yield [id('enum-value', e.name, v.name), v];
  }
}

/** Records, per section, the keys vanilla reads that the XXMI DLL no longer does. */
export function recordRemovedKeys(body: SpecBody, baseline: SpecBody): void {
  for (const section of body.sections) {
    const vanilla = baseline.sections.find(
      (s) => s.name.toLowerCase() === section.name.toLowerCase(),
    );
    if (!vanilla) continue;
    const known = new Set(section.keys.map((k) => k.name.toLowerCase()));
    section.removedKeys = vanilla.keys
      .filter((k) => !known.has(k.name.toLowerCase()))
      .map((k) => ({ name: k.name, source: k.source }));
  }
}

/** Sets `xxmi: true` on everything in `body` that the vanilla `baseline` doesn't have. */
export function flagXxmi(body: SpecBody, baseline: SpecBody): void {
  const known = new Set([...identities(baseline)].map(([key]) => key));
  for (const [key, entry] of identities(body)) entry.xxmi = !known.has(key);
}

export interface ExtractOptions {
  dllRoot: string;
  baselineRoot: string;
  /** Overrides for checkouts without git metadata (tests). */
  dllGit?: GitInfo;
  baselineGit?: GitInfo;
}

export interface ExtractResult {
  spec: Spec;
  /** Anchors missing from the baseline (expected: XXMI-only tables). */
  baselineWarnings: string[];
}

export function extractSpec(options: ExtractOptions): ExtractResult {
  const { body } = extractBody(DllSource.load(options.dllRoot), true);
  const baseline = extractBody(DllSource.load(options.baselineRoot), false);
  flagXxmi(body, baseline.body);
  recordRemovedKeys(body, baseline.body);
  const meta: SpecMeta = {
    extractorVersion: EXTRACTOR_VERSION,
    dll: options.dllGit ?? gitInfo(options.dllRoot),
    baseline: options.baselineGit ?? gitInfo(options.baselineRoot),
  };
  return { spec: { meta, ...body }, baselineWarnings: baseline.warnings };
}

/** File name ↦ contents for spec/generated/. Pure: same spec, same bytes. */
export function serializeSpec(spec: Spec): Record<string, string> {
  const json = (value: unknown): string => `${JSON.stringify(value, null, 2)}\n`;
  return {
    'meta.json': json(spec.meta),
    'sections.json': json({ sections: spec.sections, builtinSections: spec.builtinSections }),
    'commands.json': json({
      commands: spec.commands,
      resourceMembers: spec.resourceMembers,
      functions: spec.functions,
    }),
    'operators.json': json(spec.operators),
    'enums.json': json({ enums: spec.enums }),
  };
}
