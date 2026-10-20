import { findImplementation } from '@xxmi-lang/core';
import * as z from 'zod/v4';
import {
  MAX_EXCERPT_LINES,
  permalink,
  readExcerpt,
  searchSource,
  SourceError,
  treeFor,
} from '../sources.ts';
import { dllRepo } from '../repos.ts';
import { defineTool } from './define.ts';
import { ToolError } from './shared.ts';

const formatField = z.enum(['text', 'json']).optional().describe('text (default) or json');
const refField = z.string().max(80).optional().describe('branch, tag or commit; default latest');

export const findImplementationTool = defineTool({
  name: 'find_implementation',
  description:
    'Where the XXMI DLL implements an ini feature (store, Pool, ->Region, handling, ...): ranked file:line list with snippets and permalinks.',
  input: {
    feature: z.string().min(1).max(100),
    ref: refField.describe('DLL branch, tag or commit; default latest'),
    limit: z.number().int().min(1).max(50).optional(),
    format: formatField,
  },
  run(ctx, args) {
    const repo = dllRepo();
    if (!repo) throw new ToolError('the DLL repository is not tracked');
    const ref = ctx.catalog.resolve(repo, args.ref);
    const result = findImplementation(ctx.spec, args.feature, treeFor(ref.dir), args.limit ?? 20);
    const hits = result.hits.map((h) => ({ ...h, permalink: permalink(ref, h.path, h.line) }));
    const lines = [
      `DLL source: ${repo.owner}/${repo.name} @ ${ref.name} (${ref.sha.slice(0, 7)})`,
      ...result.entries.map(
        (e) => `spec: ${e.kind} ${e.name} -> ${e.source}${e.xxmi ? ' [XXMI-only]' : ''}`,
      ),
      ...(hits.length === 0 ? [`No matches for "${args.feature}" in the DLL source.`] : []),
      ...hits.map(
        (h) =>
          `${h.path}:${h.line}  ${h.role}${h.inFunction ? ` in ${h.inFunction}` : ''}\n    ${h.snippet}\n    ${h.permalink}`,
      ),
    ];
    return Promise.resolve({
      text: lines.join('\n'),
      data: { dll: { ref: ref.name, sha: ref.sha }, entries: result.entries, hits },
    });
  },
});

export const searchSourceTool = defineTool({
  name: 'search_source',
  description:
    'Regex search (ripgrep) in an indexed repo: dll, zzmi or xxmitools. Matches come with 2 lines of context.',
  input: {
    repo: z.string().min(1).max(80).describe('dll, zzmi, xxmitools or owner/name'),
    query: z.string().min(1).max(500).describe('regex; smart-case'),
    path_glob: z.string().max(200).optional().describe('e.g. DirectX11/*.cpp'),
    ref: refField,
    max_results: z.number().int().min(1).max(50).optional(),
    format: formatField,
  },
  async run(ctx, args) {
    const ref = ctx.catalog.resolve(ctx.catalog.repoByName(args.repo), args.ref);
    const result = await searchSource(ref.dir, args.query, {
      pathGlob: args.path_glob,
      maxResults: args.max_results ?? 20,
      timeoutMs: ctx.config.searchTimeoutMs,
    });
    const blocks = result.matches.map((m) => {
      const after = m.context.filter((c) => c.line > m.line).map((c) => `${c.line}- ${c.text}`);
      const before = m.context.filter((c) => c.line < m.line).map((c) => `${c.line}- ${c.text}`);
      return `${m.path}:${m.line}  ${permalink(ref, m.path, m.line)}\n${[...before, `${m.line}: ${m.text}`, ...after].join('\n')}`;
    });
    const notes = [
      ...(result.truncated ? ['(more matches exist; narrow the query or path_glob)'] : []),
      ...(result.timedOut
        ? [`(search stopped after ${ctx.config.searchTimeoutMs} ms; results are partial)`]
        : []),
      ...(result.matches.length === 0 ? ['No matches.'] : []),
    ];
    return {
      text: [
        `${ref.repo.owner}/${ref.repo.name} @ ${ref.name} (${ref.sha.slice(0, 7)})`,
        ...blocks,
        ...notes,
      ].join('\n\n'),
      data: { repo: `${ref.repo.owner}/${ref.repo.name}`, ref: ref.name, sha: ref.sha, ...result },
    };
  },
});

export const readSourceTool = defineTool({
  name: 'read_source',
  description: `Read lines of a file in an indexed repo (dll, zzmi or xxmitools), at most ${MAX_EXCERPT_LINES} lines.`,
  input: {
    repo: z.string().min(1).max(80),
    path: z
      .string()
      .min(1)
      .max(400)
      .describe('path inside the repo, e.g. DirectX11/CommandList.cpp'),
    start_line: z.number().int().min(1),
    end_line: z.number().int().min(1),
    ref: refField,
    format: formatField,
  },
  run(ctx, args) {
    const ref = ctx.catalog.resolve(ctx.catalog.repoByName(args.repo), args.ref);
    let excerpt;
    try {
      excerpt = readExcerpt(ref.dir, args.path, args.start_line, args.end_line);
    } catch (error) {
      if (error instanceof SourceError) throw new ToolError(error.message);
      throw error;
    }
    const link = permalink(ref, excerpt.path, excerpt.startLine);
    return Promise.resolve({
      text: `${excerpt.path} lines ${excerpt.startLine}-${excerpt.endLine} of ${excerpt.totalLines}  ${link}\n${excerpt.text}`,
      data: {
        repo: `${ref.repo.owner}/${ref.repo.name}`,
        ref: ref.name,
        sha: ref.sha,
        permalink: link,
        ...excerpt,
      },
    });
  },
});
