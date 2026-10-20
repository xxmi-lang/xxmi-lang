import {
  describeSnapshotSymbol,
  describeSymbol,
  listLibraries,
  renderSymbol,
  type SymbolInfo,
  type SymbolSnapshot,
} from '@xxmi-lang/core';
import * as z from 'zod/v4';
import { permalink } from '../sources.ts';
import { choosePackage, type PackageChoice } from './package.ts';
import { defineTool } from './define.ts';
import { nextCursorLine, pageOf, ToolError } from './shared.ts';

const formatField = z.enum(['text', 'json']).optional().describe('text (default) or json');
const gameField = z.string().max(20).describe('game profile, e.g. zzmi');
const refField = z.string().max(80).optional().describe('branch, tag or commit; default latest');

function packageLabel(choice: PackageChoice): string {
  const s = choice.snapshot.source;
  return `${choice.snapshot.name} @ ${choice.resolved?.name ?? 'bundled snapshot'} (${s.commit.slice(0, 7)})`;
}

export const libraryList = defineTool({
  name: 'library_list',
  description:
    'List the libraries (namespaces) of a game package with symbol counts and module docs.',
  input: {
    game: gameField,
    ref: refField,
    cursor: z.string().max(40).optional(),
    format: formatField,
  },
  async run(ctx, args) {
    const choice = await choosePackage(ctx, args.game, args.ref, { needDocs: true });
    const libs = listLibraries(choice.snapshot, ctx.lookup, choice.loaded?.pkg);
    const page = pageOf(libs, args.cursor);
    const blocks = page.items.map((lib) => {
      const counts = Object.entries(lib.counts)
        .map(([k, n]) => `${n} ${k}`)
        .join(', ');
      const doc = lib.moduleDoc
        ? `\n  ${lib.moduleDoc.text.split('\n').slice(0, 12).join('\n  ')}`
        : '';
      return `${lib.namespace}  (${counts})${lib.files ? `\n  files: ${lib.files.join(', ')}` : ''}${doc}`;
    });
    const head = [`package: ${packageLabel(choice)}`, ...(choice.note ? [choice.note] : [])];
    return {
      text: [...head, ...blocks, ...nextCursorLine(page.nextCursor)].join('\n\n'),
      data: {
        package: choice.snapshot.source,
        note: choice.note ?? null,
        libraries: page.items,
        nextCursor: page.nextCursor ?? null,
      },
    };
  },
});

function suggestions(snapshot: SymbolSnapshot, name: string): string[] {
  const last = name.split('\\').pop()?.toLowerCase() ?? '';
  if (last === '') return [];
  const names = [
    ...snapshot.sections.map((s) => s.qualifiedName),
    ...snapshot.variables.map((v) => v.key),
  ];
  return names.filter((n) => n.toLowerCase().includes(last)).slice(0, 8);
}

function linkOf(info: SymbolInfo, choice: PackageChoice): string | undefined {
  if (!choice.resolved) return undefined;
  const path = `${packagePrefix(choice)}${info.location.path}`;
  return permalink(choice.resolved, path, info.location.line);
}

/** Path of the package folder inside the repo, with a trailing slash (`ZZMI/`), or empty. */
function packagePrefix(choice: PackageChoice): string {
  const dir = choice.resolved?.repo.packageDir;
  return dir ? `${dir}/` : '';
}

export const librarySymbol = defineTool({
  name: 'library_symbol',
  description:
    'Describe a game-package library symbol (e.g. CommandList\\ZZMI\\SetTextures, $\\TTL\\alpha): location, doc comments, usage, references, inputs and source.',
  input: {
    qualified_name: z.string().min(1).max(300),
    game: gameField,
    ref: refField,
    format: formatField,
  },
  async run(ctx, args) {
    const choice = await choosePackage(ctx, args.game, args.ref, { needDocs: true });
    const info = choice.loaded
      ? describeSymbol(choice.loaded.pkg, args.qualified_name)
      : describeSnapshotSymbol(choice.snapshot, args.qualified_name);
    if (!info) {
      const near = suggestions(choice.snapshot, args.qualified_name);
      throw new ToolError(
        `No symbol "${args.qualified_name}" in ${packageLabel(choice)}.${near.length > 0 ? ` Similar: ${near.join(', ')}` : ''}`,
      );
    }
    const link = linkOf(info, choice);
    return {
      text: renderSymbol(info, {
        ...(link ? { link } : {}),
        ...(choice.note ? { note: choice.note } : {}),
      }),
      data: {
        package: choice.snapshot.source,
        packageDir: choice.resolved?.repo.packageDir ?? null,
        note: choice.note ?? null,
        symbol: info,
      },
    };
  },
});
