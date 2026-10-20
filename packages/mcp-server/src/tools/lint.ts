import { join, relative, resolve, sep } from 'node:path';
import {
  ConfigLoader,
  formatLintText,
  lintResultDocument,
  lintTexts,
  VirtualPathResolver,
  type LintConfig,
} from '@xxmi-lang/core';
import * as z from 'zod/v4';
import { choosePackage } from './package.ts';
import { defineTool } from './define.ts';
import { ToolError, type ToolContext } from './shared.ts';

/** Where the submitted files live for the linter; nothing at or below it is ever read. */
const VIRTUAL_ROOT = resolve(sep, 'xxmi-virtual');
const MAX_PATH_CHARS = 260;
const MAX_REPORTED_DIAGNOSTICS = 500;

/** xxmi.toml is never read from the server's disk. */
class NoConfig extends ConfigLoader {
  override forDirectory(): LintConfig {
    return { rules: {} };
  }
}

/** Normalizes a caller-supplied path to a clean relative `a/b.ini`, or throws. */
export function cleanVirtualPath(raw: string): string {
  const path = raw.replace(/\\/g, '/').replace(/^(\.\/)+/, '');
  const segments = path.split('/');
  const bad =
    path === '' ||
    path.length > MAX_PATH_CHARS ||
    path.includes('\0') ||
    path.startsWith('/') ||
    /^[a-zA-Z]:/.test(path) ||
    segments.some((s) => s === '' || s === '..' || s === '.');
  if (bad)
    throw new ToolError(
      `invalid path "${raw.slice(0, 80)}": use a relative path like Mods/MyMod/mod.ini`,
    );
  if (!path.toLowerCase().endsWith('.ini'))
    throw new ToolError(`"${raw.slice(0, 80)}" is not an .ini file`);
  return path;
}

export function checkLintInput(
  ctx: ToolContext,
  files: Record<string, string>,
): { path: string; text: string }[] {
  const entries = Object.entries(files);
  if (entries.length === 0) throw new ToolError('files is empty');
  if (entries.length > ctx.config.maxLintFiles) {
    throw new ToolError(`too many files: ${entries.length} (limit ${ctx.config.maxLintFiles})`);
  }
  const seen = new Set<string>();
  let bytes = 0;
  const out = entries.map(([raw, text]) => {
    const path = cleanVirtualPath(raw);
    if (seen.has(path.toLowerCase())) throw new ToolError(`duplicate path "${path}"`);
    seen.add(path.toLowerCase());
    bytes += Buffer.byteLength(text);
    return { path, text };
  });
  if (bytes > ctx.config.maxLintBytes) {
    throw new ToolError(`input too large: ${bytes} bytes (limit ${ctx.config.maxLintBytes})`);
  }
  return out;
}

export const lintIni = defineTool({
  name: 'lint_ini',
  description:
    'Lint ini text with the xxmi linter. files maps a path (e.g. Mods/MyMod/mod.ini, decides namespace) to its text. Same diagnostics as `xxmi lint --format json`; library references resolve against the indexed package.',
  input: {
    files: z
      .record(z.string(), z.string())
      .describe('{path: content}; up to 1 MB and 200 files in total'),
    game: z.string().max(20).default('zzmi').describe('game profile, e.g. zzmi'),
    package_ref: z
      .string()
      .max(80)
      .optional()
      .describe('package branch, tag or commit; default latest'),
    format: z.enum(['text', 'json']).optional().describe('text (default) or json'),
  },
  async run(ctx, args) {
    const files = checkLintInput(ctx, args.files);
    const choice = await choosePackage(ctx, args.game, args.package_ref, { needDocs: false });
    const virtual = files.map((f) => ({ path: join(VIRTUAL_ROOT, f.path), text: f.text }));

    const started = Date.now();
    const result = await lintTexts(virtual, {
      resolver: new VirtualPathResolver(virtual.map((f) => f.path)),
      snapshots: [choice.snapshot],
      configs: new NoConfig(),
      lookup: ctx.lookup,
    });
    // Sizes and counts only: the submitted text must never reach the logs.
    ctx.logger.info('lint_ini', {
      files: files.length,
      bytes: files.reduce((n, f) => n + Buffer.byteLength(f.text), 0),
      ms: Date.now() - started,
      errors: result.summary.error,
      warnings: result.summary.warning,
    });

    const pathOf = (p: string): string => relative(VIRTUAL_ROOT, p).split(sep).join('/');
    const reported = capDiagnostics(result);
    const doc = { ...lintResultDocument(reported.result, pathOf), package: choice.snapshot.source };
    const text = [
      formatLintText(reported.result, pathOf),
      ...(reported.dropped > 0 ? [`(${reported.dropped} more diagnostics not shown)`] : []),
      ...(choice.note ? [choice.note] : []),
    ].join('\n');
    return { text, data: reported.dropped > 0 ? { ...doc, truncated: reported.dropped } : doc };
  },
});

function capDiagnostics<R extends { files: { diagnostics: unknown[] }[] }>(
  result: R,
): { result: R; dropped: number } {
  let budget = MAX_REPORTED_DIAGNOSTICS;
  let dropped = 0;
  const files = result.files.map((f) => {
    const kept = f.diagnostics.slice(0, Math.max(budget, 0));
    budget -= kept.length;
    dropped += f.diagnostics.length - kept.length;
    return { ...f, diagnostics: kept };
  });
  return { result: { ...result, files }, dropped };
}
