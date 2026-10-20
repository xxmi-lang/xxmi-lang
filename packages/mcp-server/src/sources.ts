/**
 * Read-only access to indexed checkouts: a `SourceTree` for core's `findImplementation`,
 * `read_source` excerpts and ripgrep-backed `search_source`. Paths from callers are normalized
 * and must stay inside the checkout (docs/06-mcp-server.md "Security and limits").
 */
import { execFile } from 'node:child_process';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { decodeText, type SourceTree } from '@xxmi-lang/core';
import type { ResolvedRef } from './catalog.ts';
import { fullName } from './repos.ts';

export class SourceError extends Error {}

/** `rel` inside `root`, or undefined when it is absolute, has NUL, or escapes the root. */
export function safeJoin(root: string, rel: string): string | undefined {
  if (rel === '' || rel.includes('\0') || isAbsolute(rel) || /^[a-zA-Z]:/.test(rel))
    return undefined;
  const base = resolve(root);
  const full = resolve(base, rel.replace(/\\/g, '/'));
  const inside = relative(base, full);
  return inside === '' || inside.startsWith('..') || isAbsolute(inside) ? undefined : full;
}

/** A checkout as a `SourceTree`; the file list is read once. */
export class CheckoutTree implements SourceTree {
  private readonly root: string;
  private list: string[] | undefined;
  private readonly texts = new Map<string, string | undefined>();

  constructor(root: string) {
    this.root = root;
  }

  files(): string[] {
    this.list ??= walk(this.root, this.root);
    return this.list;
  }

  read(path: string): string | undefined {
    if (this.texts.has(path)) return this.texts.get(path);
    const full = safeJoin(this.root, path);
    let text: string | undefined;
    try {
      text = full ? decodeText(readFileSync(full)).text : undefined;
    } catch {
      text = undefined;
    }
    // Bounded: the DLL scan touches ~100 files, and a few trees are alive at a time.
    if (this.texts.size < 400) this.texts.set(path, text);
    return text;
  }
}

function walk(root: string, dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(root, full));
    else if (entry.isFile()) out.push(relative(root, full).split(sep).join('/'));
  }
  return out.sort();
}

const trees = new Map<string, CheckoutTree>();

/** A shared tree per checkout directory (checkouts are immutable once published). */
export function treeFor(dir: string): CheckoutTree {
  let tree = trees.get(dir);
  if (!tree) {
    tree = new CheckoutTree(dir);
    trees.set(dir, tree);
    while (trees.size > 6) {
      const oldest = trees.keys().next();
      if (oldest.done) break;
      trees.delete(oldest.value);
    }
  }
  return tree;
}

/** `https://github.com/<owner>/<repo>/blob/<sha>/<path>#L<line>` */
export function permalink(ref: ResolvedRef, path: string, line?: number): string {
  const encoded = path.split('/').map(encodeURIComponent).join('/');
  return `https://github.com/${fullName(ref.repo)}/blob/${ref.sha}/${encoded}${line ? `#L${line}` : ''}`;
}

function isFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

export const MAX_EXCERPT_LINES = 400;

export interface Excerpt {
  path: string;
  startLine: number;
  endLine: number;
  totalLines: number;
  text: string;
}

export function readExcerpt(
  root: string,
  path: string,
  startLine: number,
  endLine: number,
): Excerpt {
  if (endLine < startLine) throw new SourceError('end_line must be >= start_line');
  if (endLine - startLine + 1 > MAX_EXCERPT_LINES) {
    throw new SourceError(`at most ${MAX_EXCERPT_LINES} lines per call`);
  }
  const full = safeJoin(root, path);
  if (!full) throw new SourceError(`path "${path}" is not inside the repository`);
  if (!isFile(full)) throw new SourceError(`no such file: ${path}`);
  const lines = decodeText(readFileSync(full)).text.split(/\r?\n/);
  const last = Math.min(endLine, lines.length);
  const width = String(last).length;
  const text = lines
    .slice(startLine - 1, last)
    .map((l, i) => `${String(startLine + i).padStart(width)}| ${l}`)
    .join('\n');
  return {
    path: relative(resolve(root), full).split(sep).join('/'),
    startLine,
    endLine: last,
    totalLines: lines.length,
    text,
  };
}

export interface SearchMatch {
  path: string;
  line: number;
  text: string;
  context: { line: number; text: string }[];
}

export interface SearchResult {
  matches: SearchMatch[];
  truncated: boolean;
  timedOut: boolean;
}

interface RgMessage {
  type: string;
  data: { path?: { text: string }; lines?: { text?: string }; line_number?: number };
}

const RG_MAX_BUFFER = 8_000_000;

/** ripgrep (a linear-time regex engine, so a hostile pattern can't hang the server). */
export function searchSource(
  root: string,
  query: string,
  options: { pathGlob?: string | undefined; maxResults: number; timeoutMs: number },
): Promise<SearchResult> {
  const args = [
    '--json',
    '--no-config',
    '--smart-case',
    '--context',
    '2',
    '--max-filesize',
    '2M',
    '--max-columns',
    '300',
    '--regex-size-limit',
    '5M',
    '--max-count',
    '10',
    '-e',
    query,
  ];
  if (options.pathGlob) args.push('--glob', options.pathGlob);
  args.push('--', '.');

  return new Promise((resolvePromise, reject) => {
    execFile(
      'rg',
      args,
      { cwd: root, timeout: options.timeoutMs, maxBuffer: RG_MAX_BUFFER, killSignal: 'SIGKILL' },
      (error, stdout, stderr) => {
        const failure = error as { code?: unknown; killed?: boolean } | null;
        if (failure?.code === 'ENOENT') {
          reject(
            new SourceError('search is unavailable: ripgrep (rg) is not installed on the server'),
          );
          return;
        }
        const timedOut = failure?.killed === true;
        // rg exits 1 when nothing matched and 2 on a bad pattern (stderr explains).
        if (failure && !timedOut && failure.code === 2 && stdout === '') {
          reject(new SourceError(`invalid search: ${stderr.split('\n')[0] ?? 'regex error'}`));
          return;
        }
        resolvePromise({ ...parseRg(stdout, options.maxResults), timedOut });
      },
    );
  });
}

function parseRg(stdout: string, maxResults: number): Omit<SearchResult, 'timedOut'> {
  const matches: SearchMatch[] = [];
  let pending: { line: number; text: string }[] = [];
  let last: SearchMatch | undefined;
  for (const raw of stdout.split('\n')) {
    if (raw === '') continue;
    let msg: RgMessage;
    try {
      msg = JSON.parse(raw) as RgMessage;
    } catch {
      continue; // a line cut off by the kill
    }
    const path = msg.data.path?.text.replace(/^\.[\\/]/, '').replace(/\\/g, '/');
    const line = msg.data.line_number;
    const text = (msg.data.lines?.text ?? '').replace(/\r?\n$/, '');

    if (msg.type === 'context' && line !== undefined) {
      const after = last && last.path === path && line > last.line && line <= last.line + 2;
      if (after) last?.context.push({ line, text });
      else pending.push({ line, text });
    } else if (msg.type === 'match' && path && line !== undefined) {
      if (matches.length >= maxResults) return { matches, truncated: true };
      last = { path, line, text, context: pending.filter((c) => c.line < line) };
      pending = [];
      matches.push(last);
    } else if (msg.type === 'end') {
      pending = [];
      last = undefined;
    }
  }
  return { matches, truncated: false };
}
