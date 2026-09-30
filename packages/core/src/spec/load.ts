/**
 * Loads spec/generated/*.json and deep-merges spec/overlay/*.toml over it.
 * See docs/01-language-model.md §1 "Output schema".
 */
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse as parseToml } from 'smol-toml';
import type {
  Documented,
  Spec,
  SpecCommand,
  SpecCommands,
  SpecEnum,
  SpecEnums,
  SpecFunction,
  SpecKey,
  SpecMeta,
  SpecOperators,
  SpecOperatorToken,
  SpecResourceMember,
  SpecSection,
  SpecSections,
} from './types.ts';

/** The repository's spec/ directory (works from src/ and dist/). */
export const DEFAULT_SPEC_ROOT = fileURLToPath(new URL('../../../../spec/', import.meta.url));

export class SpecOverlayError extends Error {
  readonly problems: string[];

  constructor(problems: string[]) {
    super(`Invalid spec overlay:\n${problems.map((p) => `  - ${p}`).join('\n')}`);
    this.name = 'SpecOverlayError';
    this.problems = problems;
  }
}

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8'));
}

/** The generated spec without overlays. */
export function loadGeneratedSpec(root = DEFAULT_SPEC_ROOT): Spec {
  const dir = join(root, 'generated');
  const commands = readJson(join(dir, 'commands.json')) as SpecCommands;
  const sections = readJson(join(dir, 'sections.json')) as SpecSections;
  return {
    meta: readJson(join(dir, 'meta.json')) as SpecMeta,
    sections: sections.sections,
    builtinSections: sections.builtinSections,
    commands: commands.commands,
    resourceMembers: commands.resourceMembers,
    functions: commands.functions,
    operators: readJson(join(dir, 'operators.json')) as SpecOperators,
    enums: (readJson(join(dir, 'enums.json')) as SpecEnums).enums,
  };
}

/** The generated spec with every `overlay/*.toml` merged in file-name order. */
export function loadSpec(root = DEFAULT_SPEC_ROOT): Spec {
  const spec = loadGeneratedSpec(root);
  const dir = join(root, 'overlay');
  let files: string[];
  try {
    files = readdirSync(dir)
      .filter((f) => f.endsWith('.toml'))
      .sort();
  } catch {
    return spec; // No overlay directory: the generated spec as is.
  }
  const problems: string[] = [];
  for (const file of files) {
    let overlay: unknown;
    try {
      overlay = parseToml(readFileSync(join(dir, file), 'utf8'));
    } catch (error) {
      problems.push(`${file}: ${error instanceof Error ? error.message : String(error)}`);
      continue;
    }
    problems.push(...applyOverlay(spec, overlay, file));
  }
  if (problems.length > 0) throw new SpecOverlayError(problems);
  return spec;
}

type Table = Record<string, unknown>;

const DOC_FIELDS = ['doc', 'example', 'since', 'deprecated', 'note'] as const;

function isTable(value: unknown): value is Table {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

const same = (a: string, b: string): boolean => a.toLowerCase() === b.toLowerCase();

/**
 * Merges one parsed overlay into `spec` (mutating it) and returns the problems found.
 *
 * Overlay tables address generated entries by name, case-insensitively:
 * `[sections.<name>]`, `[sections.<name>.keys.<key>]`, `[commands.<name>]`, `[members.<name>]`,
 * `[functions.<name>]`, `[operators."<token>"]`, `[enums.<name>]`. They may set documentation
 * fields (`doc`, `example`, `since`, `deprecated`, `note`), plus `valueType` on keys and `args`
 * on members. An entry that doesn't exist in the generated spec is an error unless it has
 * `source = "manual"` and a `note` saying why; it is then added.
 */
export function applyOverlay(spec: Spec, overlay: unknown, file: string): string[] {
  const problems: string[] = [];
  const problem = (path: string, message: string): void => {
    problems.push(`${file}: ${path}: ${message}`);
  };
  if (!isTable(overlay)) return [`${file}: not a TOML table`];

  for (const [group, entries] of Object.entries(overlay)) {
    if (!isTable(entries)) {
      problem(group, 'expected a table of entries');
      continue;
    }
    for (const [name, raw] of Object.entries(entries)) {
      const path = `${group}.${name}`;
      if (!isTable(raw)) {
        problem(path, 'expected a table');
        continue;
      }
      switch (group) {
        case 'sections':
          mergeSection(spec, name, raw, path, problem);
          break;
        case 'commands':
          mergeNamed(spec.commands, name, raw, path, problem, [], (e) =>
            manualEntry<SpecCommand>(e, path, problem, ['kind'], {
              name,
              kind: e.kind as SpecCommand['kind'],
              match: 'exact',
              values: [],
            }),
          );
          break;
        case 'members':
          mergeNamed(spec.resourceMembers, name, raw, path, problem, ['args'], (e) =>
            manualEntry<SpecResourceMember>(e, path, problem, [], { name, args: [] }),
          );
          break;
        case 'functions':
          mergeNamed(spec.functions, name, raw, path, problem, [], (e) =>
            manualEntry<SpecFunction>(e, path, problem, [], { name }),
          );
          break;
        case 'enums':
          mergeNamed(spec.enums, name, raw, path, problem, [], (e) =>
            manualEntry<SpecEnum>(e, path, problem, [], { name, values: [] }),
          );
          break;
        case 'operators': {
          const token = spec.operators.tokens.find((t) => t.token === name);
          if (token) assign(token, raw, path, problem, []);
          else {
            const added = manualEntry<SpecOperatorToken>(raw, path, problem, [], { token: name });
            if (added) spec.operators.tokens.push(added);
          }
          break;
        }
        default:
          problem(group, 'unknown overlay group');
      }
    }
  }
  return problems;
}

type Problem = (path: string, message: string) => void;

function mergeSection(spec: Spec, name: string, raw: Table, path: string, problem: Problem): void {
  const { keys, ...fields } = raw;
  let section = spec.sections.find((s) => same(s.name, name));
  if (section) {
    assign(section, fields, path, problem, []);
  } else {
    section = manualEntry<SpecSection>(fields, path, problem, ['kind'], {
      name,
      prefix: fields.prefix === true,
      kind: fields.kind as SpecSection['kind'],
      allowsBareLines: fields.allowsBareLines === true,
      allowsDuplicateKeys: fields.allowsDuplicateKeys === true,
      keys: [],
      dynamicKeys: [],
    });
    if (!section) return;
    spec.sections.push(section);
  }
  if (keys === undefined) return;
  if (!isTable(keys)) {
    problem(`${path}.keys`, 'expected a table of keys');
    return;
  }
  for (const [keyName, keyRaw] of Object.entries(keys)) {
    const keyPath = `${path}.keys.${keyName}`;
    if (!isTable(keyRaw)) {
      problem(keyPath, 'expected a table');
      continue;
    }
    mergeNamed(section.keys, keyName, keyRaw, keyPath, problem, ['valueType'], (e) =>
      manualEntry<SpecKey>(e, keyPath, problem, ['valueType'], {
        name: keyName,
        valueType: String(e.valueType),
      }),
    );
  }
}

function mergeNamed<T extends { name: string }>(
  list: T[],
  name: string,
  raw: Table,
  path: string,
  problem: Problem,
  extraFields: string[],
  create: (raw: Table) => T | undefined,
): void {
  const entry = list.find((e) => same(e.name, name));
  if (entry) {
    assign(entry, raw, path, problem, extraFields);
    return;
  }
  const added = create(raw);
  if (added) list.push(added);
}

/** Copies documentation (and `extraFields`) from `raw` onto an existing generated entry. */
function assign(
  target: object,
  raw: Table,
  path: string,
  problem: Problem,
  extraFields: string[],
): void {
  const allowed = new Set<string>([...DOC_FIELDS, ...extraFields]);
  for (const [field, value] of Object.entries(raw)) {
    if (field === 'source') {
      problem(path, '`source` can only be set on new (`source = "manual"`) entries');
    } else if (!allowed.has(field)) {
      problem(path, `field \`${field}\` can't be set by an overlay`);
    } else if (field === 'args' ? !isStringArray(value) : typeof value !== 'string') {
      problem(
        path,
        `\`${field}\` must be ${field === 'args' ? 'an array of strings' : 'a string'}`,
      );
    } else {
      (target as Record<string, unknown>)[field] = value;
    }
  }
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((v) => typeof v === 'string');
}

/** Builds an entry that only exists in the overlay; undefined (with problems) if not allowed. */
function manualEntry<T extends object>(
  raw: Table,
  path: string,
  problem: Problem,
  required: string[],
  base: Omit<T, 'source' | 'xxmi' | keyof Documented>,
): T | undefined {
  if (raw.source !== 'manual') {
    problem(path, 'not in the generated spec (add `source = "manual"` and a `note` if intended)');
    return undefined;
  }
  let ok = true;
  if (typeof raw.note !== 'string' || raw.note.trim() === '') {
    problem(path, 'manual entries need a `note` saying why they exist');
    ok = false;
  }
  for (const field of required) {
    if (raw[field] === undefined) {
      problem(path, `manual entries of this kind need \`${field}\``);
      ok = false;
    }
  }
  if (!ok) return undefined;
  const entry: Record<string, unknown> = { ...base, source: 'manual', xxmi: raw.xxmi === true };
  for (const field of DOC_FIELDS) {
    if (typeof raw[field] === 'string') entry[field] = raw[field];
  }
  return entry as T;
}
