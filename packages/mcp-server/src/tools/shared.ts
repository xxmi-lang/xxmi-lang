/** Plumbing shared by the tool handlers: context, output shape, pagination, timeouts. */
import type { Spec, SpecLookup, SymbolSnapshot } from '@xxmi-lang/core';
import type { Catalog } from '../catalog.ts';
import type { ServerConfig } from '../config.ts';
import type { Logger } from '../logger.ts';
import type { PackageCache } from '../packages.ts';

export interface ToolContext {
  config: ServerConfig;
  catalog: Catalog;
  packages: PackageCache;
  logger: Logger;
  spec: Spec;
  lookup: SpecLookup;
  /** Snapshots bundled with core, served until the first index is ready. */
  bundled: (game: string) => SymbolSnapshot | undefined;
}

/** What a handler returns: the compact text and the structured value behind it. */
export interface ToolOutput {
  text: string;
  data: unknown;
}

/** A failure the caller can act on (bad input, unknown ref, warming up). */
export class ToolError extends Error {}

export type OutputFormat = 'text' | 'json';

export const PAGE_SIZE = 100;

export function pageOf<T>(
  items: T[],
  cursor: string | undefined,
): { items: T[]; nextCursor?: string } {
  let offset = 0;
  if (cursor) {
    const match = /^o(\d{1,9})$/.exec(Buffer.from(cursor, 'base64url').toString());
    if (!match?.[1]) throw new ToolError('invalid cursor');
    offset = Number(match[1]);
  }
  const slice = items.slice(offset, offset + PAGE_SIZE);
  const next = offset + PAGE_SIZE;
  return next < items.length
    ? { items: slice, nextCursor: Buffer.from(`o${next}`).toString('base64url') }
    : { items: slice };
}

export function withTimeout<T>(promise: Promise<T>, ms: number, what: string): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(new ToolError(`${what} timed out after ${ms} ms`));
    }, ms);
  });
  return Promise.race([promise, timeout]).finally(() => {
    clearTimeout(timer);
  });
}

export function nextCursorLine(nextCursor: string | undefined): string[] {
  return nextCursor ? [`next cursor: ${nextCursor} (pass as "cursor" for more)`] : [];
}
