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
import type { ResolvedSpec } from '../specs/registry.ts';
import { dllRepo } from '../repos.ts';
import { defineTool } from './define.ts';
import {
  dllRefField,
  nextCursorLine,
  pageOf,
  resolveSpec,
  ToolError,
  withSpec,
  type ToolContext,
} from './shared.ts';

const formatField = z.enum(['text', 'json']).optional().describe('text (default) or json');
const kindField = z.enum(SPEC_ENTRY_KINDS);

interface Located {
  path: string;
  line: number;
  permalink: string;
  commit: string;
}

/** Where the entry's DLL symbol sits in the indexed source of the DLL the spec came from. */
function locate(ctx: ToolContext, entry: SpecEntry, spec: ResolvedSpec): Located | undefined {
  const repo = dllRepo();
  if (!repo || !ctx.catalog.isReady(repo) || entry.source === 'manual') return undefined;
  const ref = ctx.catalog.resolve(repo, spec.source === 'extracted' ? spec.dllSha : undefined);
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
    'Look up a 3DMigoto/XXMI ini section, key, command, operator, ->member, function or enum: docs, args, XXMI-only flag, example and DLL source location. Members carry a target (resource or shader); commands with form "value" are written as the value, not as a key.',
  input: {
    name: z.string().min(1).max(200).describe('e.g. store, Resource, hash, ->Region, <<'),
    kind: kindField.optional().describe('narrow to one kind'),
    section: z.string().max(100).optional().describe('for keys: the section that reads it'),
    target: z.enum(['resource', 'shader']).optional().describe('for members: resource or shader'),
    dll_ref: dllRefField,
    format: formatField,
  },
  async run(ctx, args) {
    const resolved = await resolveSpec(ctx, args.dll_ref);
    const entries = lookupSpec(
      resolved.spec,
      {
        name: args.name,
        ...(args.kind ? { kind: args.kind } : {}),
        ...(args.section ? { section: args.section } : {}),
        ...(args.target ? { target: args.target } : {}),
      },
      resolved.lookup,
    );
    const located = entries.map((e) => ({ ...e, dll: locate(ctx, e, resolved) }));
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
    return withSpec({ text, data: { entries: located } }, resolved);
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
    dll_ref: dllRefField,
    format: formatField,
  },
  async run(ctx, args) {
    const resolved = await resolveSpec(ctx, args.dll_ref);
    let items;
    try {
      items = listSpec(resolved.spec, args.kind, args.section, resolved.lookup);
    } catch (error) {
      throw new ToolError(error instanceof Error ? error.message : String(error));
    }
    const page = pageOf(items, args.cursor);
    const lines = page.items.map(
      (i) =>
        `${i.name}${i.target ? ` [${i.target}]` : ''}${i.form ? ' [value form]' : ''}${i.xxmi ? ' [XXMI-only]' : ''}${i.summary ? ` - ${i.summary}` : ''}`,
    );
    return withSpec(
      {
        text: [...lines, ...nextCursorLine(page.nextCursor)].join('\n'),
        data: { items: page.items, nextCursor: page.nextCursor ?? null },
      },
      resolved,
    );
  },
});
