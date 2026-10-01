/**
 * Editor navigation over the workspace model: what's under the cursor, definition, references,
 * rename and outline. Frontends (LSP, MCP) only translate these results to their protocols.
 */
import { dirname } from 'node:path';
import type { IniFile, Reference, SectionModel, VariableDeclaration } from '../model/types.ts';
import type { Position, Range } from '../parser/index.ts';
import {
  explicitNamespace,
  globalVariableKey,
  type SectionSymbol,
  type VariableSymbol,
} from '../workspace/symbols.ts';
import type { Workspace } from '../workspace/workspace.ts';

export interface Location {
  /** Host path. */
  path: string;
  range: Range;
}

export type Target =
  | { kind: 'section'; symbol: SectionSymbol; range: Range }
  | { kind: 'variable'; symbol: VariableSymbol; range: Range }
  | { kind: 'path'; path: string; range: Range };

export function contains(range: Range, pos: Position): boolean {
  const afterStart =
    pos.line > range.start.line ||
    (pos.line === range.start.line && pos.character >= range.start.character);
  const beforeEnd =
    pos.line < range.end.line ||
    (pos.line === range.end.line && pos.character <= range.end.character);
  return afterStart && beforeEnd;
}

/** Offset of `pos` in `text` (UTF-16, like positions). */
export function offsetAt(text: string, pos: Position): number {
  let line = 0;
  let offset = 0;
  while (line < pos.line) {
    const next = text.indexOf('\n', offset);
    if (next < 0) return text.length;
    offset = next + 1;
    line++;
  }
  return Math.min(offset + pos.character, text.length);
}

function positionAt(text: string, offset: number): Position {
  let line = 0;
  let lineStart = 0;
  for (let i = text.indexOf('\n'); i >= 0 && i < offset; i = text.indexOf('\n', i + 1)) {
    line++;
    lineStart = i + 1;
  }
  return { line, character: offset - lineStart };
}

function rangeOf(text: string, start: number, end: number): Range {
  return { start: positionAt(text, start), end: positionAt(text, end) };
}

/** The section symbol declared by a section of a loaded file. */
function ownSymbol(workspace: Workspace, file: IniFile, index: number): SectionSymbol | undefined {
  const section = file.sections[index];
  if (!section?.kind) return undefined;
  return workspace.symbols
    .sectionsNamed(section.qualifiedName)
    .find((s) => 'file' in s.origin && s.origin.file === file && s.origin.section === index);
}

function resolveRef(workspace: Workspace, file: IniFile, ref: Reference): Target | undefined {
  if (ref.kind === 'variable') {
    const symbol = workspace.symbols.resolveVariable(ref, file);
    return symbol ? { kind: 'variable', symbol, range: ref.span.range } : undefined;
  }
  const symbol = workspace.symbols.resolveSection(ref.kind, ref.text, file.namespace);
  return symbol ? { kind: 'section', symbol, range: ref.span.range } : undefined;
}

/** What the cursor is on: a reference, a declaration, or an include/filename path. */
export function targetAt(workspace: Workspace, file: IniFile, pos: Position): Target | undefined {
  const ref = file.references.find((r) => contains(r.span.range, pos));
  if (ref) return resolveRef(workspace, file, ref);

  const sectionIndex = file.sections.findIndex((s) => contains(s.name.span.range, pos));
  if (sectionIndex >= 0) {
    const symbol = ownSymbol(workspace, file, sectionIndex);
    const section = file.sections[sectionIndex];
    if (symbol && section) return { kind: 'section', symbol, range: section.name.span.range };
  }

  const declaration = file.variables.find((v) => contains(v.span.range, pos));
  if (declaration) {
    const symbol = variableSymbolOf(workspace, file, declaration);
    if (symbol) return { kind: 'variable', symbol, range: declaration.span.range };
  }

  for (const section of file.sections) {
    const kind = section.spec?.name;
    for (const line of section.lines) {
      if (!line.value?.text || !contains(line.value.span.range, pos)) continue;
      const key = line.key?.text.toLowerCase();
      const isInclude = kind === 'Include' && (key === 'include' || key === 'include_recursive');
      const isFilename = (kind === 'Resource' || kind === 'Pool') && key === 'filename';
      if (!isInclude && !isFilename) continue;
      const root = workspace.packageRoot(file);
      const found =
        workspace.resolver.resolve(dirname(file.path), line.value.text) ??
        (isFilename && root ? workspace.resolver.resolve(root, line.value.text) : undefined);
      if (found) return { kind: 'path', path: found.path, range: line.value.span.range };
    }
  }
  return undefined;
}

function variableSymbolOf(
  workspace: Workspace,
  file: IniFile,
  declaration: VariableDeclaration,
): VariableSymbol | undefined {
  if (declaration.scope === 'local') {
    return {
      key: declaration.name.toLowerCase(),
      persist: false,
      locked: false,
      origin: { file, section: declaration.section },
      declaration,
    };
  }
  const key = globalVariableKey(declaration.name, file.namespace);
  return workspace.symbols
    .allVariables()
    .find((v) => v.key === key && v.declaration === declaration);
}

/** Where a target is declared. Snapshot and built-in symbols have no location. */
export function definition(workspace: Workspace, target: Target): Location | undefined {
  if (target.kind === 'path')
    return {
      path: target.path,
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
    };
  const origin = target.symbol.origin;
  if (!('file' in origin)) return undefined;
  if (target.kind === 'variable') {
    const declaration = target.symbol.declaration;
    return declaration ? { path: origin.file.path, range: declaration.span.range } : undefined;
  }
  const section = origin.file.sections[origin.section];
  return section ? { path: origin.file.path, range: section.name.span.range } : undefined;
}

function sameTarget(a: Target, b: Target): boolean {
  if (a.kind === 'section' && b.kind === 'section') {
    return (
      a.symbol.kind === b.symbol.kind &&
      a.symbol.qualifiedName.toLowerCase() === b.symbol.qualifiedName.toLowerCase()
    );
  }
  if (a.kind === 'variable' && b.kind === 'variable') {
    if (a.symbol.declaration?.scope === 'local' || b.symbol.declaration?.scope === 'local') {
      return a.symbol.declaration === b.symbol.declaration;
    }
    return a.symbol.key === b.symbol.key;
  }
  return false;
}

/** Every reference to `target` in the workspace, plus its declaration if asked. */
export function references(
  workspace: Workspace,
  target: Target,
  includeDeclaration: boolean,
): Location[] {
  const out: Location[] = [];
  if (includeDeclaration) {
    const decl = definition(workspace, target);
    if (decl && target.kind !== 'path') out.push(decl);
  }
  if (target.kind === 'path') return out;
  for (const file of workspace.files.values()) {
    for (const ref of file.references) {
      if ((ref.kind === 'variable') !== (target.kind === 'variable')) continue;
      const resolved = resolveRef(workspace, file, ref);
      if (resolved && sameTarget(resolved, target))
        out.push({ path: file.path, range: ref.span.range });
    }
  }
  return out;
}

export interface TextEdit {
  range: Range;
  newText: string;
}

export type RenameResult =
  | { ok: true; range: Range; placeholder: string; edits: Map<string, TextEdit[]> }
  | { ok: false; message: string };

/** Offset (in `text`) where the renameable name part of a reference or header starts. */
function namePartStart(text: string, workspace: Workspace, isVariable: boolean): number {
  const lastSlash = text.lastIndexOf('\\');
  if (lastSlash >= 0) return lastSlash + 1;
  if (isVariable) return 1; // after `$`
  return workspace.lookup.sectionPrefix(text)?.length ?? 0;
}

function isPackageFile(workspace: Workspace, file: IniFile): boolean {
  if (file.namespace === '') return true; // d3dx.ini: global sections other mods call
  const root = workspace.packageRoot(file);
  if (!root) return false;
  return (
    file.path
      .slice(root.length + 1)
      .split(/[\\/]/)[0]
      ?.toLowerCase() === 'core'
  );
}

const SECTION_NAME = /^[^\s\\[\]=,;$]+$/;
const VARIABLE_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * Renames a section (and every `run =`, `ref`, `Prefix\N\Name` reference) or a variable (`$x`,
 * `$\N\x`). Only the name part changes; prefixes and namespaces are kept as written. Symbols
 * declared in the package or a snapshot can't be renamed from a mod.
 */
export function rename(
  workspace: Workspace,
  file: IniFile,
  pos: Position,
  newName?: string,
): RenameResult {
  const target = targetAt(workspace, file, pos);
  if (!target || target.kind === 'path') return { ok: false, message: 'Nothing to rename here.' };
  const origin = target.symbol.origin;
  if (!('file' in origin)) {
    return {
      ok: false,
      message: `\`${target.kind === 'section' ? target.symbol.qualifiedName : target.symbol.key}\` comes from the ${origin.snapshot === 'builtin' ? 'DLL' : `${origin.snapshot} library snapshot`} and can't be renamed.`,
    };
  }
  if (isPackageFile(workspace, origin.file) && !workspace.targets.has(origin.file.path)) {
    return {
      ok: false,
      message: `This symbol is declared in the package (${origin.file.path}); rename it there.`,
    };
  }

  const isVariable = target.kind === 'variable';
  const edits = new Map<string, TextEdit[]>();
  const add = (path: string, text: string, start: number, end: number, newText: string): void => {
    const list = edits.get(path) ?? [];
    list.push({ range: rangeOf(text, start, end), newText });
    edits.set(path, list);
  };

  // The declaration's name part.
  let placeholder = '';
  let placeholderRange: Range = target.range;
  const declFile = origin.file;
  if (target.kind === 'section') {
    const section: SectionModel | undefined = declFile.sections[origin.section];
    if (!section) return { ok: false, message: 'Declaration not found.' };
    const start = section.name.span.start + namePartStart(section.name.text, workspace, false);
    placeholder = declFile.text.slice(start, section.name.span.end);
    if (newName !== undefined)
      add(declFile.path, declFile.text, start, section.name.span.end, newName);
    if (declFile === file && contains(section.name.span.range, pos))
      placeholderRange = rangeOf(file.text, start, section.name.span.end);
  } else {
    const declaration = target.symbol.declaration;
    if (!declaration) return { ok: false, message: 'Declaration not found.' };
    const start = declaration.span.start + namePartStart(declaration.name, workspace, true);
    placeholder = declFile.text.slice(start, declaration.span.end);
    if (newName !== undefined)
      add(declFile.path, declFile.text, start, declaration.span.end, newName);
    if (declFile === file && contains(declaration.span.range, pos))
      placeholderRange = rangeOf(file.text, start, declaration.span.end);
  }

  for (const other of workspace.files.values()) {
    for (const ref of other.references) {
      if ((ref.kind === 'variable') !== isVariable) continue;
      const resolved = resolveRef(workspace, other, ref);
      if (!resolved || !sameTarget(resolved, target)) continue;
      const start = ref.span.start + namePartStart(ref.text, workspace, isVariable);
      const end = ref.span.start + ref.text.length;
      if (newName !== undefined) add(other.path, other.text, start, end, newName);
      if (other === file && contains(ref.span.range, pos))
        placeholderRange = rangeOf(other.text, start, end);
    }
  }

  if (newName !== undefined) {
    const valid = isVariable ? VARIABLE_NAME.test(newName) : SECTION_NAME.test(newName);
    if (!valid) {
      return {
        ok: false,
        message: isVariable
          ? 'Variable names are letters, digits and `_`, not starting with a digit.'
          : "Section names can't contain spaces, `\\`, `[`, `]`, `=`, `,`, `;` or `$`.",
      };
    }
    const conflict =
      target.kind === 'section'
        ? workspace.symbols.sectionsNamed(
            target.symbol.qualifiedName.slice(
              0,
              target.symbol.qualifiedName.length - placeholder.length,
            ) + newName,
          ).length > 0
        : workspace.symbols
            .allVariables()
            .some(
              (v) =>
                v.key ===
                target.symbol.key.slice(0, target.symbol.key.length - placeholder.length) +
                  newName.toLowerCase(),
            );
    if (conflict && newName.toLowerCase() !== placeholder.toLowerCase()) {
      return { ok: false, message: `\`${newName}\` is already declared in that namespace.` };
    }
  }
  return { ok: true, range: placeholderRange, placeholder, edits };
}

export interface OutlineSymbol {
  name: string;
  kind: string;
  range: Range;
  selectionRange: Range;
  children: OutlineSymbol[];
}

/** Sections grouped by kind (docs/03-lsp.md `documentSymbol`). */
export function outline(file: IniFile): OutlineSymbol[] {
  const groups = new Map<string, OutlineSymbol>();
  for (const section of file.sections) {
    const group = section.spec?.name ?? 'Unknown';
    const range = {
      start: section.header.content.span.range.start,
      end: (section.lines.at(-1) ?? section.header).content.span.range.end,
    };
    const symbol: OutlineSymbol = {
      name: section.name.text,
      kind: group,
      range,
      selectionRange: section.name.span.range,
      children: [],
    };
    const existing = groups.get(group);
    if (existing) {
      existing.children.push(symbol);
      existing.range = { start: existing.range.start, end: range.end };
    } else {
      groups.set(group, {
        name: group,
        kind: 'group',
        range,
        selectionRange: section.name.span.range,
        children: [symbol],
      });
    }
  }
  return [...groups.values()];
}

/** Namespace named explicitly in a reference, for hover/completion. */
export { explicitNamespace };
