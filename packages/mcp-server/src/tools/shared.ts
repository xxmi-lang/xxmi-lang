/** Plumbing shared by the tool handlers: context, output shape, pagination, timeouts. */
import type { SymbolSnapshot } from '@xxmi-lang/core';
import * as z from 'zod/v4';
import type { Catalog } from '../catalog.ts';
import type { ServerConfig } from '../config.ts';
import type { Logger } from '../logger.ts';
import type { PackageCache } from '../packages.ts';
import type { ResolvedSpec, SpecRegistry } from '../specs/registry.ts';

export interface ToolContext {
  config: ServerConfig;
  catalog: Catalog;
  packages: PackageCache;
  logger: Logger;
  /** Every spec the server can answer with; tools pick one through `resolveSpec`. */
  specs: SpecRegistry;
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

export const dllRefField = z
  .string()
  .max(80)
  .optional()
  .describe(
    'DLL version the spec comes from: latest-release (default), latest-prerelease, head, a tag, a commit prefix or bundled',
  );

/** The spec a tool answers with, and the line and block that tell the caller which one it was. */
export function resolveSpec(ctx: ToolContext, dllRef: string | undefined): Promise<ResolvedSpec> {
  return ctx.specs.resolve(dllRef);
}

export function specBlock(resolved: ResolvedSpec): Record<string, unknown> {
  return {
    source: resolved.source,
    dllRef: resolved.dllRef,
    dllSha: resolved.dllSha,
    tag: resolved.tag ?? null,
    kind: resolved.kind ?? null,
    dllCommitDate: resolved.dllCommitDate,
    ...(resolved.note ? { note: resolved.note } : {}),
  };
}

/** `spec: XXMI-Libs-Package v1.2.3 (release, a1b2c3d)` or `spec: bundled (b0bff3f); requested …`. */
export function specLine(resolved: ResolvedSpec): string {
  const sha = resolved.dllSha.slice(0, 7);
  if (resolved.source === 'extracted') {
    return `spec: XXMI-Libs-Package ${resolved.name} (${resolved.kind}, ${sha})`;
  }
  return `spec: bundled (${sha})${resolved.fallback ? `; ${resolved.fallback}` : ''}`;
}

/** Adds the `spec` block to the structured result and the one-line summary to the text. */
export function withSpec(output: ToolOutput, resolved: ResolvedSpec): ToolOutput {
  const data = output.data as Record<string, unknown>;
  return {
    text: `${output.text}\n${specLine(resolved)}`,
    data: { ...data, spec: specBlock(resolved) },
  };
}

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
