/**
 * "Where does the DLL implement this?" (MCP `find_implementation`). Spec entries carry the
 * DLL source symbol they were extracted from (`CommandList.cpp:ParseCommandListGeneralCommands`);
 * this resolves that symbol to lines in a DLL source tree, plus other mentions of the same
 * keyword.
 */
import type { Spec } from '../spec/types.ts';
import { lookupSpec, type SpecEntry } from './spec.ts';

/** A read-only view of a DLL source checkout, paths `/`-separated and relative to its root. */
export interface SourceTree {
  files(): string[];
  read(path: string): string | undefined;
}

export type HitRole = 'definition' | 'table' | 'literal' | 'mention';

export interface ImplementationHit {
  path: string;
  /** 1-based. */
  line: number;
  role: HitRole;
  snippet: string;
  /** The function the line is in (a guess from the nearest unindented signature above it). */
  inFunction?: string;
  /** The spec entry this hit was found for. */
  entry?: string;
}

export interface ImplementationResult {
  feature: string;
  entries: SpecEntry[];
  hits: ImplementationHit[];
}

const ROLE_RANK: Record<HitRole, number> = { definition: 0, table: 0, literal: 1, mention: 2 };
const SOURCE_FILE = /\.(?:cpp|h|hpp|c)$/i;
const MAX_FILE_BYTES = 2_000_000;
const MENTION_CAP = 10;
const SNIPPET_CHARS = 200;

function splitSource(source: string): { file: string; symbol: string } | undefined {
  const match = /^(.+?\.(?:cpp|h|hpp|c)):(.+)$/i.exec(source);
  return match?.[1] && match[2] ? { file: match[1], symbol: match[2] } : undefined;
}

function resolveFile(tree: SourceTree, file: string): string | undefined {
  const lower = file.toLowerCase();
  return tree
    .files()
    .filter((f) => f.toLowerCase() === lower || f.toLowerCase().endsWith(`/${lower}`))
    .sort((a, b) => a.length - b.length)[0];
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

const SIGNATURE = /^[A-Za-z_][\w:<>*&,\s]*?\b([A-Za-z_]\w*(?:::~?\w+)*)\s*\(/;
const NOT_A_SIGNATURE = /^(?:if|for|while|switch|return|else|do|case|using|typedef)\b/;
const SIGNATURE_LOOKBACK = 600;

function enclosingFunction(lines: string[], index: number): string | undefined {
  for (let i = index - 1; i >= Math.max(0, index - SIGNATURE_LOOKBACK); i--) {
    const text = lines[i] ?? '';
    if (NOT_A_SIGNATURE.test(text) || text.trimEnd().endsWith(';')) continue;
    const name = SIGNATURE.exec(text)?.[1];
    if (name) return name;
  }
  return undefined;
}

const snippetOf = (line: string): string => line.trim().slice(0, SNIPPET_CHARS);

function symbolHits(
  path: string,
  lines: string[],
  symbol: string,
  entry: string,
): ImplementationHit[] {
  if (symbol === 'literal') return [];
  const name = escapeRegExp(symbol);
  const definition = new RegExp(`^\\S.*\\b${name}\\s*\\(`);
  const table = new RegExp(`^\\S.*\\b${name}\\b\\s*(?:\\[|=|\\{|$)`);
  const hits: ImplementationHit[] = [];
  lines.forEach((text, i) => {
    const role: HitRole | undefined =
      definition.test(text) && !text.trimEnd().endsWith(';')
        ? 'definition'
        : table.test(text) && !text.includes('(')
          ? 'table'
          : undefined;
    if (role) hits.push({ path, line: i + 1, role, snippet: snippetOf(text), entry });
  });
  return hits.slice(0, 3);
}

function literalPattern(name: string): RegExp {
  return new RegExp(`L?"${escapeRegExp(name)}"`, 'i');
}

function literalHits(
  path: string,
  lines: string[],
  name: string,
  role: HitRole,
  entry: string,
): ImplementationHit[] {
  const pattern = literalPattern(name);
  const hits: ImplementationHit[] = [];
  lines.forEach((text, i) => {
    if (!pattern.test(text)) return;
    const inFunction = enclosingFunction(lines, i);
    hits.push({
      path,
      line: i + 1,
      role,
      snippet: snippetOf(text),
      ...(inFunction ? { inFunction } : {}),
      entry,
    });
  });
  return hits;
}

function readLines(tree: SourceTree, path: string): string[] | undefined {
  const text = tree.read(path);
  return text === undefined || text.length > MAX_FILE_BYTES ? undefined : text.split(/\r?\n/);
}

function entryHits(entry: SpecEntry, tree: SourceTree): ImplementationHit[] {
  const source = splitSource(entry.source);
  const path = source ? resolveFile(tree, source.file) : undefined;
  const lines = path ? readLines(tree, path) : undefined;
  if (!source || !path || !lines) return [];
  const label = `${entry.kind} ${entry.name}`;
  const name = entry.name.replace(/^->/, '');
  return [
    ...symbolHits(path, lines, source.symbol, label),
    ...literalHits(path, lines, name, 'literal', label),
  ];
}

/** The best DLL source location for a spec entry (its symbol's definition, else a literal). */
export function locateSpecEntry(entry: SpecEntry, tree: SourceTree): ImplementationHit | undefined {
  return rankHits(entryHits(entry, tree))[0];
}

function rankHits(hits: ImplementationHit[]): ImplementationHit[] {
  return hits
    .map((hit, order) => ({ hit, order }))
    .sort((a, b) => ROLE_RANK[a.hit.role] - ROLE_RANK[b.hit.role] || a.order - b.order)
    .map((x) => x.hit);
}

function mentionHits(tree: SourceTree, name: string, skip: Set<string>): ImplementationHit[] {
  const hits: ImplementationHit[] = [];
  for (const path of tree.files().filter((f) => SOURCE_FILE.test(f))) {
    const lines = readLines(tree, path);
    if (!lines) continue;
    for (const hit of literalHits(path, lines, name, 'mention', name)) {
      if (!skip.has(`${hit.path}:${hit.line}`)) hits.push(hit);
      if (hits.length >= MENTION_CAP) return hits;
    }
  }
  return hits;
}

/** Word-boundary text matches, for features that aren't spec entries. */
function wordHits(tree: SourceTree, word: string): ImplementationHit[] {
  const pattern = new RegExp(`\\b${escapeRegExp(word)}\\b`, 'i');
  const hits: ImplementationHit[] = [];
  for (const path of tree.files().filter((f) => SOURCE_FILE.test(f))) {
    const lines = readLines(tree, path);
    lines?.forEach((text, i) => {
      if (hits.length < MENTION_CAP * 2 && pattern.test(text)) {
        hits.push({ path, line: i + 1, role: 'mention', snippet: snippetOf(text) });
      }
    });
  }
  return hits;
}

/** Ranked DLL source locations for an ini feature (`store`, `Pool`, `->Region`, `handling`, …). */
export function findImplementation(
  spec: Spec,
  feature: string,
  tree: SourceTree,
  limit = 20,
): ImplementationResult {
  const entries = lookupSpec(spec, { name: feature });
  const hits = entries.flatMap((e) => entryHits(e, tree));
  const bare = feature.trim().replace(/^->/, '');
  if (entries.length === 0) hits.push(...wordHits(tree, bare));
  else if (bare.length >= 3) {
    const seen = new Set(hits.map((h) => `${h.path}:${h.line}`));
    hits.push(...mentionHits(tree, bare, seen));
  }
  return { feature, entries, hits: rankHits(hits).slice(0, limit) };
}
