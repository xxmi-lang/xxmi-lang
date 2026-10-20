/**
 * Library lookups for `xxmi query symbol|library` and the MCP `library_symbol` / `library_list`
 * tools: what a package's public symbols are, where they live, and what documents them.
 */
import { relative, sep } from 'node:path';
import type { IniFile, SymbolKind } from '../model/types.ts';
import type { SpecLookup } from '../spec/lookup.ts';
import {
  explicitNamespace,
  type SectionSymbol,
  type SymbolSnapshot,
  type VariableSymbol,
} from '../workspace/symbols.ts';
import type { Workspace } from '../workspace/workspace.ts';
import { commentBlocks, docAbove, splitLines, type DocComment } from './docs.ts';

export interface SymbolLocation {
  /** Relative to the package root, `/`-separated. */
  path: string;
  /** 1-based. */
  line: number;
}

export interface SymbolUsage extends SymbolLocation {
  text: string;
}

export interface SymbolMember extends SymbolLocation {
  kind: SymbolKind;
  name: string;
}

export interface SymbolReference {
  kind: SymbolKind;
  text: string;
  access: 'read' | 'write';
}

export interface SymbolInfo {
  kind: SymbolKind;
  /** As the DLL stores it, e.g. `CommandList\ZZMI\SetTextures`. */
  qualifiedName: string;
  location: SymbolLocation;
  /** Only the location is known: the symbol comes from a bundled snapshot. */
  snapshotOnly: boolean;
  doc?: DocComment;
  /** The comment block at the top of the declaring file (usually a `Usage:` block). */
  moduleDoc?: SymbolUsage;
  /** The section body (or the declaration line), capped at `SOURCE_LINE_CAP` lines. */
  source?: string;
  /** What the section reads, writes and runs. */
  references: SymbolReference[];
  /**
   * Comment blocks elsewhere in the package that mention the symbol by its full name: usage
   * examples in library headers, since most libraries document inputs there.
   */
  usage: SymbolUsage[];
  /** Other symbols in the same namespace: for libraries, the inputs and outputs they share. */
  namespaceMembers: SymbolMember[];
}

export const SOURCE_LINE_CAP = 80;
const USAGE_CAP = 5;
const USAGE_LINE_CAP = 30;
const MEMBER_CAP = 80;

/** A package loaded into a workspace, with the root its paths are shown relative to. */
export interface LoadedPackage {
  workspace: Workspace;
  root: string;
}

const relPath = (root: string, path: string): string => relative(root, path).split(sep).join('/');

function lookupSymbol(
  workspace: Workspace,
  name: string,
): { section: SectionSymbol } | { variable: VariableSymbol } | undefined {
  const lower = name.toLowerCase();
  const section = workspace.symbols
    .allSections()
    .find((s) => s.qualifiedName.toLowerCase() === lower);
  if (section) return { section };
  const variable = workspace.symbols.allVariables().find((v) => v.key === lower);
  return variable ? { variable } : undefined;
}

function referencesOf(file: IniFile, sectionIndex: number): SymbolReference[] {
  const seen = new Set<string>();
  const out: SymbolReference[] = [];
  for (const r of file.references) {
    if (r.section !== sectionIndex) continue;
    const key = `${r.kind}|${r.text.toLowerCase()}|${r.access}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ kind: r.kind, text: r.text, access: r.access });
  }
  return out;
}

function moduleDocOf(file: IniFile, root: string): SymbolUsage | undefined {
  const firstSection = file.sections[0]?.header.line ?? Infinity;
  const block = commentBlocks(splitLines(file.text)).find((b) => b.line - 1 < firstSection);
  return block ? { path: relPath(root, file.path), line: block.line, text: block.text } : undefined;
}

function usageOf(pkg: LoadedPackage, qualifiedName: string, own: SymbolLocation): SymbolUsage[] {
  const needle = qualifiedName.toLowerCase();
  const out: SymbolUsage[] = [];
  for (const file of pkg.workspace.files.values()) {
    const path = relPath(pkg.root, file.path);
    for (const block of commentBlocks(splitLines(file.text))) {
      if (!block.text.toLowerCase().includes(needle)) continue;
      const lines = block.text.split('\n');
      // The block directly above the declaration is its own doc, not a usage example.
      if (path === own.path && block.endLine === own.line - 1) continue;
      out.push({ path, line: block.line, text: lines.slice(0, USAGE_LINE_CAP).join('\n') });
      if (out.length >= USAGE_CAP) return out;
    }
  }
  return out;
}

function membersOf(pkg: LoadedPackage, lookup: SpecLookup, qualifiedName: string): SymbolMember[] {
  const namespace = explicitNamespace(qualifiedName, lookup)?.toLowerCase();
  if (!namespace) return [];
  const members: SymbolMember[] = [];
  for (const file of pkg.workspace.files.values()) {
    for (const s of file.sections) {
      if (!s.kind || s.qualifiedName.toLowerCase() === qualifiedName.toLowerCase()) continue;
      if (explicitNamespace(s.qualifiedName, lookup)?.toLowerCase() !== namespace) continue;
      members.push({
        kind: s.kind,
        name: s.qualifiedName,
        path: relPath(pkg.root, file.path),
        line: s.header.line + 1,
      });
    }
  }
  for (const v of pkg.workspace.symbols.allVariables()) {
    if (explicitNamespace(v.key, lookup)?.toLowerCase() !== namespace || !('file' in v.origin))
      continue;
    members.push({
      kind: 'variable',
      name: v.declaration?.name ?? v.key,
      path: relPath(pkg.root, v.origin.file.path),
      line: (v.declaration?.span.range.start.line ?? 0) + 1,
    });
  }
  return members.slice(0, MEMBER_CAP);
}

function sectionInfo(pkg: LoadedPackage, symbol: SectionSymbol): SymbolInfo | undefined {
  if (!('file' in symbol.origin)) return undefined;
  const { file, section: index } = symbol.origin;
  const section = file.sections[index];
  if (!section) return undefined;
  const lines = splitLines(file.text);
  const location = { path: relPath(pkg.root, file.path), line: section.header.line + 1 };
  const doc = docAbove(lines, section.header.line);
  const module = moduleDocOf(file, pkg.root);
  const end = file.sections[index + 1]?.header.line ?? lines.length;
  const body = lines.slice(
    section.header.line,
    Math.min(end, section.header.line + SOURCE_LINE_CAP),
  );
  return {
    kind: symbol.kind,
    qualifiedName: symbol.qualifiedName,
    location,
    snapshotOnly: false,
    ...(doc ? { doc } : {}),
    ...(module ? { moduleDoc: module } : {}),
    source: body.join('\n').trimEnd(),
    references: referencesOf(file, index),
    usage: [],
    namespaceMembers: [],
  };
}

function variableInfo(pkg: LoadedPackage, symbol: VariableSymbol): SymbolInfo | undefined {
  if (!('file' in symbol.origin) || !symbol.declaration) return undefined;
  const { file } = symbol.origin;
  const line = symbol.declaration.span.range.start.line;
  const lines = splitLines(file.text);
  const doc = docAbove(lines, line);
  const module = moduleDocOf(file, pkg.root);
  return {
    kind: 'variable',
    qualifiedName: symbol.declaration.name,
    location: { path: relPath(pkg.root, file.path), line: line + 1 },
    snapshotOnly: false,
    ...(doc ? { doc } : {}),
    ...(module ? { moduleDoc: module } : {}),
    source: lines[line]?.trim() ?? '',
    references: [],
    usage: [],
    namespaceMembers: [],
  };
}

/**
 * Describes the symbol with this qualified name (`CommandList\ZZMI\SetTextures`,
 * `$\TTL\alpha`) in a loaded package, or undefined when the package doesn't declare it.
 */
export function describeSymbol(pkg: LoadedPackage, name: string): SymbolInfo | undefined {
  const found = lookupSymbol(pkg.workspace, name.trim());
  if (!found) return undefined;
  const info =
    'section' in found ? sectionInfo(pkg, found.section) : variableInfo(pkg, found.variable);
  if (!info) return undefined;
  const key = 'section' in found ? info.qualifiedName : found.variable.key;
  info.usage = usageOf(pkg, key, info.location);
  info.namespaceMembers = membersOf(pkg, pkg.workspace.lookup, key);
  return info;
}

/** The location a snapshot records for `name`; no docs (the snapshot doesn't carry them). */
export function describeSnapshotSymbol(
  snapshot: SymbolSnapshot,
  name: string,
): SymbolInfo | undefined {
  const lower = name.trim().toLowerCase();
  const section = snapshot.sections.find((s) => s.qualifiedName.toLowerCase() === lower);
  if (section) {
    return {
      kind: section.kind,
      qualifiedName: section.qualifiedName,
      location: { path: section.path, line: section.line },
      snapshotOnly: true,
      references: [],
      usage: [],
      namespaceMembers: [],
    };
  }
  const variable = snapshot.variables.find((v) => v.key === lower);
  if (!variable) return undefined;
  return {
    kind: 'variable',
    qualifiedName: variable.key,
    location: { path: variable.path, line: variable.line },
    snapshotOnly: true,
    references: [],
    usage: [],
    namespaceMembers: [],
  };
}

export interface LibraryInfo {
  namespace: string;
  counts: Partial<Record<SymbolKind, number>>;
  /** Files of the namespace and the doc block at the top of the first one that has one. */
  files?: string[];
  moduleDoc?: SymbolUsage;
}

/** Namespaces that declare symbols, with symbol counts; docs and files when the package is loaded. */
export function listLibraries(
  snapshot: SymbolSnapshot,
  lookup: SpecLookup,
  pkg?: LoadedPackage,
): LibraryInfo[] {
  const byNamespace = new Map<string, LibraryInfo>();
  const entries = [
    ...snapshot.sections.map((s) => ({ kind: s.kind, name: s.qualifiedName })),
    ...snapshot.variables.map((v) => ({ kind: 'variable' as const, name: v.key })),
  ];
  for (const e of entries) {
    const original = snapshot.namespaces.find(
      (n) => n.toLowerCase() === explicitNamespace(e.name, lookup)?.toLowerCase(),
    );
    if (!original) continue;
    const lib = byNamespace.get(original) ?? { namespace: original, counts: {} };
    lib.counts[e.kind] = (lib.counts[e.kind] ?? 0) + 1;
    byNamespace.set(original, lib);
  }
  const libs = [...byNamespace.values()].sort((a, b) => a.namespace.localeCompare(b.namespace));
  if (pkg) for (const lib of libs) addFiles(lib, pkg);
  return libs;
}

function addFiles(lib: LibraryInfo, pkg: LoadedPackage): void {
  const files = [...pkg.workspace.files.values()].filter(
    (f) => f.namespace.toLowerCase() === lib.namespace.toLowerCase(),
  );
  lib.files = files.map((f) => relPath(pkg.root, f.path)).sort();
  for (const file of files) {
    const doc = moduleDocOf(file, pkg.root);
    if (doc) {
      lib.moduleDoc = doc;
      return;
    }
  }
}
