import {
  listSpec,
  locateSpecEntry,
  lookupSpec,
  renderSpecEntry,
  SPEC_ENTRY_KINDS,
  type SpecEntry,
} from '@xxmi-lang/core';
import * as z from 'zod/v4';
import { permalink, treeFor } from '../sources.ts';
import { dllRepo } from '../repos.ts';
import { defineTool } from './define.ts';
import { nextCursorLine, pageOf, ToolError, type ToolContext } from './shared.ts';

const formatField = z.enum(['text', 'json']).optional().describe('text (default) or json');
const kindField = z.enum(SPEC_ENTRY_KINDS);

/** `latest` or the commit (7+ characters) the bundled spec was extracted from. */
function checkDllRef(ctx: ToolContext, ref: string | undefined): void {
  if (!ref || ref === 'latest') return;
  const commit = ctx.spec.meta.dll.commit;
  if (ref.length >= 7 && commit.startsWith(ref.toLowerCase())) return;
  throw new ToolError(
    `This server's spec was extracted from XXMI-Libs-Package ${commit} ("latest"); other dll_ref values are not served.`,
  );
}

interface Located {
  path: string;
  line: number;
  permalink: string;
  commit: string;
}

/** Where the entry's DLL symbol sits in the indexed DLL source, when that checkout exists. */
function locate(ctx: ToolContext, entry: SpecEntry): Located | undefined {
  const repo = dllRepo();
  if (!repo || !ctx.catalog.isReady(repo) || entry.source === 'manual') return undefined;
  const ref = ctx.catalog.resolve(repo);
  const hit = locateSpecEntry(entry, treeFor(ref.dir));
  return hit
    ? {
        path: hit.path,
        line: hit.line,
        permalink: permalink(ref, hit.path, hit.line),
        commit: ref.sha,
      }
    : undefined;
}

export const specLookup = defineTool({
  name: 'spec_lookup',
  description:
    'Look up a 3DMigoto/XXMI ini section, key, command, operator, ->member, function or enum: docs, args, XXMI-only flag, example and DLL source location.',
  input: {
    name: z.string().min(1).max(200).describe('e.g. store, Resource, hash, ->Region, <<'),
    kind: kindField.optional().describe('narrow to one kind'),
    section: z.string().max(100).optional().describe('for keys: the section that reads it'),
    dll_ref: z.string().max(80).optional().describe('only "latest" (default) is served'),
    format: formatField,
  },
  run(ctx, args) {
    checkDllRef(ctx, args.dll_ref);
    const entries = lookupSpec(
      ctx.spec,
      {
        name: args.name,
        ...(args.kind ? { kind: args.kind } : {}),
        ...(args.section ? { section: args.section } : {}),
      },
      ctx.lookup,
    );
    const located = entries.map((e) => ({ ...e, dll: locate(ctx, e) }));
    const text =
      entries.length === 0
        ? `No spec entry named "${args.name}"${args.kind ? ` of kind ${args.kind}` : ''}. Use spec_list to browse names.`
        : located
            .map((e) =>
              renderSpecEntry(
                e,
                e.dll && { path: e.dll.path, line: e.dll.line, link: e.dll.permalink },
              ),
            )
            .join('\n\n');
    return Promise.resolve({ text, data: { entries: located } });
  },
});

export const specList = defineTool({
  name: 'spec_list',
  description:
    'List names with one-line docs of one spec kind, e.g. all keys valid in [Resource*].',
  input: {
    kind: kindField,
    section: z.string().max(100).optional().describe('required for kind=key, e.g. Resource'),
    cursor: z.string().max(40).optional(),
    format: formatField,
  },
  run(ctx, args) {
    let items;
    try {
      items = listSpec(ctx.spec, args.kind, args.section, ctx.lookup);
    } catch (error) {
      throw new ToolError(error instanceof Error ? error.message : String(error));
    }
    const page = pageOf(items, args.cursor);
    const lines = page.items.map(
      (i) => `${i.name}${i.xxmi ? ' [XXMI-only]' : ''}${i.summary ? ` - ${i.summary}` : ''}`,
    );
    return Promise.resolve({
      text: [...lines, ...nextCursorLine(page.nextCursor)].join('\n'),
      data: { items: page.items, nextCursor: page.nextCursor ?? null },
    });
  },
});
