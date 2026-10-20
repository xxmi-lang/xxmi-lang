/**
 * Lint results as text and as the versioned JSON document (packages/cli/schema/lint-output.v1.json),
 * shared by the CLI and the hosted MCP server so both print the same thing.
 */
import type { LintResult } from './engine.ts';

/** Shows a host path to users, e.g. relative to the cwd. */
export type PathFormatter = (path: string) => string;

/** Diagnostics as `path:line:col  severity  XM201  message`, then a summary line. */
export function formatLintText(result: LintResult, pathOf: PathFormatter): string {
  const lines: string[] = [];
  for (const file of result.files) {
    for (const d of file.diagnostics) {
      const { line, character } = d.range.start;
      const fix = d.fix ? '  [fix available]' : '';
      lines.push(
        `${pathOf(file.path)}:${line + 1}:${character + 1}  ${d.severity}  ${d.id}  ${d.message}${fix}`,
      );
    }
  }
  const s = result.summary;
  lines.push(
    `${s.files} file${s.files === 1 ? '' : 's'}: ${s.error} error${s.error === 1 ? '' : 's'}, ${s.warning} warning${s.warning === 1 ? '' : 's'}, ${s.info} info, ${s.hint} hint${s.hint === 1 ? '' : 's'}`,
  );
  return lines.join('\n');
}

/** The `version: 1` JSON document of `xxmi lint --format json`. */
export function lintResultDocument(result: LintResult, pathOf: PathFormatter): object {
  return {
    version: 1,
    files: result.files.map((f) => ({
      path: pathOf(f.path),
      encoding: f.encoding,
      diagnostics: f.diagnostics.map((d) => ({
        id: d.id,
        severity: d.severity,
        message: d.message,
        range: d.range,
        ...(d.fix ? { fix: d.fix } : {}),
      })),
    })),
    summary: result.summary,
  };
}
