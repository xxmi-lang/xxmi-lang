/**
 * Hover text and completion items (docs/03-lsp.md phase 1), built from the merged spec
 * (generated + overlay docs) and the workspace symbols.
 */
import { dirname } from 'node:path';
import type { IniFile, LineModel, SectionModel } from '../model/types.ts';
import type { Position, Range } from '../parser/index.ts';
import type { Documented, Sourced, SpecKey, SpecSection } from '../spec/types.ts';
import type { SymbolOrigin } from '../workspace/symbols.ts';
import type { Workspace } from '../workspace/workspace.ts';
import { contains, offsetAt, targetAt } from './navigation.ts';

export interface Hover {
  /** Markdown. */
  contents: string;
  range: Range;
}

function badge(entry: Sourced): string {
  return entry.xxmi ? ' · **XXMI-only**' : '';
}

function docs(entry: Documented): string {
  const parts: string[] = [];
  if (entry.doc) parts.push(entry.doc);
  if (entry.example) parts.push(`\`\`\`ini\n${entry.example}\n\`\`\``);
  if (entry.deprecated) parts.push(`**Deprecated:** ${entry.deprecated}`);
  return parts.join('\n\n');
}

function source(entry: Sourced): string {
  return entry.source === 'manual' ? '' : `\n\n<sub>DLL: \`${entry.source}\`</sub>`;
}

/** The contiguous `;` comment block above `line`, or else right below it (ZZMI's `; Input:` style). */
export function docComment(file: IniFile, line: number, below = false): string | undefined {
  const byLine = new Map(file.comments.map((c) => [c.line, c.content.text]));
  const collect = (start: number, step: number): string[] => {
    const lines: string[] = [];
    for (let n = start; byLine.has(n); n += step) lines.push(byLine.get(n) ?? '');
    return step < 0 ? lines.reverse() : lines;
  };
  let lines = collect(line - 1, -1);
  if (lines.length === 0 && below) lines = collect(line + 1, 1);
  if (lines.length === 0) return undefined;
  return lines.map((l) => l.replace(/^;;?\s?/, '')).join('\n');
}

function originText(origin: SymbolOrigin): string {
  if ('file' in origin) return '';
  if (origin.snapshot === 'builtin') return 'Built into the DLL.';
  return `From the ${origin.snapshot} library snapshot: \`${origin.path}:${origin.line}\`.`;
}

function sectionAt(file: IniFile, pos: Position): SectionModel | undefined {
  return file.sections.find(
    (s) =>
      contains(s.header.content.span.range, pos) ||
      s.lines.some((l) => contains(l.content.span.range, pos)),
  );
}

function lineAt(section: SectionModel, pos: Position): LineModel | undefined {
  return section.lines.find((l) => l.line === pos.line);
}

/** Hover for the symbol, key, command or section under the cursor. */
export function hover(workspace: Workspace, file: IniFile, pos: Position): Hover | undefined {
  const { lookup } = workspace;
  const target = targetAt(workspace, file, pos);
  if (target?.kind === 'section') {
    const { symbol } = target;
    const origin = symbol.origin;
    let doc: string | undefined;
    if ('file' in origin) {
      const section = origin.file.sections[origin.section];
      if (section) doc = docComment(origin.file, section.header.line, true);
    }
    const parts = [`\`\`\`ini\n[${symbol.qualifiedName}]\n\`\`\``, doc, originText(origin)].filter(
      Boolean,
    );
    return { contents: parts.join('\n\n'), range: target.range };
  }
  if (target?.kind === 'variable') {
    const { symbol } = target;
    const decl = symbol.declaration;
    const flags = [
      decl?.scope ?? 'global',
      symbol.persist ? 'persist' : '',
      symbol.locked ? 'locked' : '',
    ]
      .filter(Boolean)
      .join(' ');
    const origin = symbol.origin;
    let initial = '';
    let doc: string | undefined;
    if ('file' in origin && decl) {
      const line = origin.file.text.split('\n')[decl.span.range.start.line]?.trim();
      initial = line ? `\`\`\`ini\n${line}\n\`\`\`` : '';
      doc = docComment(origin.file, decl.span.range.start.line);
    }
    const header = `**${flags}** \`${decl?.name ?? symbol.key}\``;
    return {
      contents: [header, initial, doc, originText(origin)].filter(Boolean).join('\n\n'),
      range: target.range,
    };
  }

  const section = sectionAt(file, pos);
  if (!section) return undefined;
  if (contains(section.name.span.range, pos) && section.spec) {
    return { contents: sectionDoc(section.spec), range: section.name.span.range };
  }
  const line = lineAt(section, pos);
  if (!line || !section.spec) return undefined;
  const keyRange = line.key?.span.range ?? line.content.span.range;
  if (!contains(keyRange, pos)) return valueHover(workspace, line, section.spec, pos);
  const keyName = line.key?.text ?? line.content.text.split(/\s/)[0] ?? '';
  const key = lookup.key(
    section.spec,
    section.spec.kind === 'commandlist' ? (line.command ?? keyName) : keyName,
  );
  if (key) return { contents: keyDoc(key, section.spec), range: keyRange };
  if (section.spec.kind === 'commandlist') {
    const command = lookup.command(line.command ?? keyName);
    if (command) {
      const args =
        command.argCount !== undefined
          ? ` (${command.argCount} argument${command.argCount === 1 ? '' : 's'})`
          : '';
      const values = command.values.length
        ? `\n\nSpecial values: ${command.values.map((v) => `\`${v.name}${v.match === 'prefix' ? '…' : ''}\``).join(', ')}`
        : '';
      return {
        contents: `**${command.name}** — ${command.kind} command${args}${badge(command)}\n\n${docs(command)}${values}${source(command)}`,
        range: keyRange,
      };
    }
  }
  return undefined;
}

function sectionDoc(spec: SpecSection): string {
  const kind = spec.kind === 'commandlist' ? 'command-list section' : 'section';
  return `**[${spec.name}${spec.prefix ? '…' : ''}]** — ${kind}${badge(spec)}\n\n${docs(spec)}${source(spec)}`.trim();
}

function keyDoc(key: SpecKey, section: SpecSection): string {
  return `**${key.name}** in [${section.name}${section.prefix ? '…' : ''}] — \`${key.valueType}\`${badge(key)}\n\n${docs(key)}${source(key)}`.trim();
}

/** Operator or function docs for a token in a value. */
function valueHover(
  workspace: Workspace,
  line: LineModel,
  spec: SpecSection,
  pos: Position,
): Hover | undefined {
  const value = line.value;
  if (!value || !contains(value.span.range, pos)) return undefined;
  const col = pos.character - value.span.range.start.character;
  const word = /[A-Za-z_]\w*/g;
  for (let m = word.exec(value.text); m; m = word.exec(value.text)) {
    if (col < m.index || col > m.index + m[0].length) continue;
    const range = {
      start: { line: line.line, character: value.span.range.start.character + m.index },
      end: { line: line.line, character: value.span.range.start.character + m.index + m[0].length },
    };
    const fn = workspace.lookup.spec.functions.find(
      (f) => f.name.toLowerCase() === m[0].toLowerCase(),
    );
    if (fn && value.text[m.index + m[0].length] === '(') {
      return {
        contents:
          `**${fn.name}(…)** — expression function${badge(fn)}\n\n${docs(fn)}${source(fn)}`.trim(),
        range,
      };
    }
    const key = workspace.lookup.key(spec, line.command ?? line.key?.text ?? '');
    const table = /enum:(\w+)/.exec(key?.valueType ?? '')?.[1];
    if (table)
      return { contents: `Value of \`${key?.name ?? ''}\`: one of \`enum:${table}\``, range };
  }
  for (const token of [...workspace.lookup.spec.operators.tokens].sort(
    (a, b) => b.token.length - a.token.length,
  )) {
    const at = value.text.indexOf(token.token, Math.max(0, col - token.token.length));
    if (at >= 0 && at <= col && col <= at + token.token.length) {
      const level = workspace.lookup.spec.operators.precedence.find((p) =>
        p.operators.includes(token.token),
      );
      const prec = level
        ? `\n\nPrecedence level ${level.level} (\`${level.group}\`, ${level.associativity}-associative${level.unary ? ', unary' : ''}); 0 binds tightest.`
        : '';
      const range = {
        start: { line: line.line, character: value.span.range.start.character + at },
        end: {
          line: line.line,
          character: value.span.range.start.character + at + token.token.length,
        },
      };
      return {
        contents:
          `**\`${token.token}\`** operator${badge(token)}${prec}\n\n${docs(token)}${source(token)}`.trim(),
        range,
      };
    }
  }
  return undefined;
}

export type CompletionKind =
  'section' | 'key' | 'command' | 'keyword' | 'value' | 'variable' | 'reference' | 'file';

export interface CompletionItem {
  label: string;
  kind: CompletionKind;
  detail?: string;
  documentation?: string;
  /** Range the item replaces (the word being typed). */
  range: Range;
}

/** Completions at the cursor, from its context on the line. */
export function completions(workspace: Workspace, file: IniFile, pos: Position): CompletionItem[] {
  const { lookup } = workspace;
  const lineText = file.text.split('\n')[pos.line]?.replace(/\r$/, '') ?? '';
  const before = lineText.slice(0, pos.character);
  const wordStart = (pattern: RegExp): number => {
    const m = pattern.exec(before);
    return m ? pos.character - m[0].length : pos.character;
  };
  const range = (start: number): Range => ({
    start: { line: pos.line, character: start },
    end: pos,
  });

  // `[Prefix…` → section kinds.
  if (/^\s*\[[^\]]*$/.test(before)) {
    const r = range(before.indexOf('[') + 1);
    return lookup.spec.sections.map((s) => ({
      label: s.name,
      kind: 'section' as const,
      detail: `${s.kind === 'commandlist' ? 'command-list section' : 'section'}${s.prefix ? ' (prefix)' : ''}${s.xxmi ? ' · XXMI-only' : ''}`,
      documentation: docs(s),
      range: r,
    }));
  }

  const offset = offsetAt(file.text, pos);
  const section = [...file.sections].reverse().find((s) => s.chunk.start <= offset);
  const spec = section?.spec;
  const eq = before.indexOf('=');

  // Key position: keys of the section, plus commands and flow keywords in command lists.
  if (eq < 0) {
    if (before.trimStart().startsWith('$'))
      return variableItems(workspace, file, section, before, pos);
    if (!spec) return [];
    const r = range(wordStart(/[\w$]*$/));
    const items: CompletionItem[] = spec.keys.map((k) => ({
      label: k.name,
      kind: 'key',
      detail: `${k.valueType}${k.xxmi ? ' · XXMI-only' : ''}`,
      documentation: docs(k),
      range: r,
    }));
    if (spec.kind === 'commandlist') {
      for (const c of lookup.spec.commands) {
        if (c.kind === 'prefix' || c.match === 'prefix') continue;
        items.push({
          label: c.name,
          kind: c.kind === 'flow' || c.kind === 'declaration' ? 'keyword' : 'command',
          detail: `${c.kind}${c.xxmi ? ' · XXMI-only' : ''}`,
          documentation: docs(c),
          range: r,
        });
      }
    }
    return items;
  }

  // Value position.
  const key = before
    .slice(0, eq)
    .trim()
    .toLowerCase()
    .replace(/^(pre|post)\s+/, '');
  const valueBefore = before.slice(eq + 1);
  if (/\$[\w\\]*$/.test(valueBefore)) return variableItems(workspace, file, section, before, pos);
  if (spec && (key === 'filename' || key === 'include' || key === 'include_recursive')) {
    return fileItems(workspace, file, valueBefore, pos);
  }
  const keySpec = spec ? lookup.key(spec, key) : undefined;
  const table = /enum:(\w+)/.exec(keySpec?.valueType ?? '')?.[1];
  if (table) {
    const r = range(wordStart(/\w*$/));
    return (lookup.enumTable(table)?.values ?? []).map((v) => ({
      label: v.name,
      kind: 'value' as const,
      detail: `enum:${table}`,
      range: r,
    }));
  }
  const refMatch =
    /(CommandList|CustomShader|BuiltInCommandList|BuiltInCustomShader|Resource|Pool)[\w.\\]*$/i.exec(
      valueBefore,
    );
  if (refMatch || key === 'run' || key === 'checktextureoverride') {
    const start = refMatch ? pos.character - refMatch[0].length : wordStart(/[\w.\\]*$/);
    return referenceItems(workspace, file, range(start), key === 'run');
  }
  if (spec?.kind === 'commandlist') {
    const command = lookup.command(key);
    if (command?.values.length) {
      const r = range(wordStart(/\w*$/));
      return command.values.map((v) => ({
        label: v.name,
        kind: 'value' as const,
        detail: `${command.name} value`,
        range: r,
      }));
    }
  }
  return [];
}

function variableItems(
  workspace: Workspace,
  file: IniFile,
  section: SectionModel | undefined,
  before: string,
  pos: Position,
): CompletionItem[] {
  const m = /\$[\w\\]*$/.exec(before);
  const r: Range = {
    start: { line: pos.line, character: pos.character - (m?.[0].length ?? 0) },
    end: pos,
  };
  const items = new Map<string, CompletionItem>();
  const sectionIndex = section ? file.sections.indexOf(section) : -1;
  for (const v of file.variables) {
    if (v.scope === 'local' && v.section === sectionIndex)
      items.set(v.name.toLowerCase(), {
        label: v.name,
        kind: 'variable',
        detail: 'local',
        range: r,
      });
  }
  const own = file.namespace.toLowerCase();
  for (const v of workspace.symbols.allVariables()) {
    // `$\ns\name`: offer the short form in the file's own namespace.
    const short =
      own !== '' && v.key.startsWith(`$\\${own}\\`) ? `$${v.key.slice(own.length + 3)}` : v.key;
    const flags = ['global', v.persist ? 'persist' : '', v.locked ? 'locked' : '']
      .filter(Boolean)
      .join(' ');
    if (!items.has(short))
      items.set(short, { label: short, kind: 'variable', detail: flags, range: r });
  }
  return [...items.values()];
}

function referenceItems(
  workspace: Workspace,
  file: IniFile,
  range: Range,
  callable: boolean,
): CompletionItem[] {
  const own = file.namespace.toLowerCase();
  const items = new Map<string, CompletionItem>();
  const kinds = callable
    ? ['commandlist', 'customshader']
    : ['commandlist', 'customshader', 'resource', 'pool'];
  for (const s of workspace.symbols.allSections()) {
    if (!kinds.includes(s.kind)) continue;
    const q = s.qualifiedName;
    const prefix = workspace.lookup.sectionPrefix(q) ?? '';
    const inOwn = own !== '' && q.toLowerCase().startsWith(`${prefix.toLowerCase()}\\${own}\\`);
    const label = inOwn ? prefix + q.slice(prefix.length + own.length + 2) : q;
    if (!items.has(label.toLowerCase()))
      items.set(label.toLowerCase(), { label, kind: 'reference', detail: s.kind, range });
  }
  for (const b of workspace.lookup.spec.builtinSections) {
    if (!items.has(b.name.toLowerCase()))
      items.set(b.name.toLowerCase(), {
        label: b.name,
        kind: 'reference',
        detail: 'built into the DLL',
        range,
      });
  }
  return [...items.values()];
}

function fileItems(
  workspace: Workspace,
  file: IniFile,
  valueBefore: string,
  pos: Position,
): CompletionItem[] {
  const typed = valueBefore.trimStart();
  const slash = Math.max(typed.lastIndexOf('/'), typed.lastIndexOf('\\'));
  const dirPart = slash >= 0 ? typed.slice(0, slash) : '';
  const base = dirPart
    ? workspace.resolver.resolve(dirname(file.path), dirPart)?.path
    : dirname(file.path);
  if (!base) return [];
  const r: Range = {
    start: { line: pos.line, character: pos.character - (typed.length - slash - 1) },
    end: pos,
  };
  return (workspace.resolver.list(base) ?? [])
    .filter((n) => !n.startsWith('.'))
    .map((name) => ({ label: name, kind: 'file' as const, range: r }));
}
