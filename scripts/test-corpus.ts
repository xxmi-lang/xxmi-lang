/**
 * Parses and lints every .ini under corpus/ and under a game package checkout. Prints the
 * parse-failure rate with the first error per failing file, and diagnostic counts per rule
 * compared against the baseline (a jump in one rule usually means a false-positive regression).
 *
 *   XXMI_PACKAGE_SRC=../ZZMI-Package pnpm test:corpus [paths…] [--json] [--strict] [--update-baseline]
 *
 * Roots: corpus/ (if present), each path in XXMI_PACKAGE_SRC (separated like PATH), and any
 * paths given as arguments.
 *
 * --update-baseline  store the counts in corpus/.baseline.json under "parse" and "lint"
 * --strict           exit 1 if any file fails to parse or has a lint error
 * --json             print a JSON report instead of text
 *
 * Exit codes: 0 ok, 1 more parse failures or more diagnostics of some rule than the baseline
 * (or, with --strict, any parse failure or lint error), 2 usage error, 3 lint crashed.
 */
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, delimiter, join, relative, resolve } from 'node:path';
import { lintPaths, parseFile, type FileParseResult } from '../packages/core/src/index.ts';

const REPO = resolve(import.meta.dirname, '..');
const CORPUS = join(REPO, 'corpus');
const BASELINE = join(CORPUS, '.baseline.json');
const GRAMMAR_SOURCE = join(REPO, 'packages/core/grammar/SOURCE.json');
const SKIP_DIRS = new Set(['.git', 'node_modules', '.xxmi-backup']);

interface Root {
  label: string;
  dir: string;
}

interface FileReport {
  path: string;
  encoding: FileParseResult['encoding'];
  issues: number;
  first?: { line: number; character: number; message: string };
}

interface RootReport {
  label: string;
  dir: string;
  files: number;
  failed: number;
  failureRate: number;
  nonUtf8: number;
  failures: FileReport[];
  /** Diagnostics per rule id. */
  lint: Record<string, number>;
  lintErrors: number;
  /** Set when linting threw: a bug (tolerant-by-default is a hard rule). */
  crash?: string;
}

interface LintBaseline {
  updatedAt: string;
  roots: Record<string, Record<string, number>>;
}

interface ParseBaseline {
  updatedAt: string;
  grammarCommit: string | null;
  roots: Record<string, { files: number; failed: number; failureRate: number }>;
}

const args = process.argv.slice(2);
const flags = new Set(args.filter((a) => a.startsWith('--')));
for (const flag of flags) {
  if (!['--json', '--strict', '--update-baseline'].includes(flag)) {
    console.error(`Unknown option ${flag}`);
    process.exit(2);
  }
}

function collectRoots(): Root[] {
  const roots: Root[] = [];
  if (existsSync(CORPUS)) roots.push({ label: 'corpus', dir: CORPUS });
  for (const dir of (process.env.XXMI_PACKAGE_SRC ?? '').split(delimiter).filter(Boolean)) {
    roots.push({ label: `package:${basename(resolve(dir))}`, dir: resolve(dir) });
  }
  for (const dir of args.filter((a) => !a.startsWith('--'))) {
    roots.push({ label: `path:${basename(resolve(dir))}`, dir: resolve(dir) });
  }
  return roots;
}

function listIni(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory() && !SKIP_DIRS.has(entry.name)) out.push(...listIni(full));
    else if (entry.isFile() && entry.name.toLowerCase().endsWith('.ini')) out.push(full);
  }
  return out.sort();
}

/** Paths as users see them: relative to the cwd, with `/`. */
function display(path: string): string {
  return relative(process.cwd(), path).replaceAll('\\', '/');
}

async function checkRoot(root: Root): Promise<RootReport> {
  const files = listIni(root.dir);
  const failures: FileReport[] = [];
  let nonUtf8 = 0;
  for (const file of files) {
    const result = await parseFile(file);
    result.tree.delete();
    if (result.encoding === 'unknown') nonUtf8++;
    const [first] = result.issues;
    if (first) {
      failures.push({
        path: display(file),
        encoding: result.encoding,
        issues: result.issues.length,
        first: {
          line: first.range.start.line + 1,
          character: first.range.start.character + 1,
          message: first.message,
        },
      });
    }
  }
  const report: RootReport = {
    label: root.label,
    dir: display(root.dir),
    files: files.length,
    failed: failures.length,
    failureRate: files.length === 0 ? 0 : failures.length / files.length,
    nonUtf8,
    failures,
    lint: {},
    lintErrors: 0,
  };
  try {
    const result = await lintPaths([root.dir]);
    for (const f of result.files) {
      for (const d of f.diagnostics) report.lint[d.id] = (report.lint[d.id] ?? 0) + 1;
    }
    report.lintErrors = result.summary.error;
  } catch (error) {
    report.crash = error instanceof Error ? (error.stack ?? error.message) : String(error);
  }
  report.lint = Object.fromEntries(
    Object.entries(report.lint).sort(([a], [b]) => (a < b ? -1 : 1)),
  );
  return report;
}

function readJson(path: string): Record<string, unknown> {
  return existsSync(path)
    ? (JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>)
    : {};
}

function percent(rate: number): string {
  return `${(rate * 100).toFixed(1)}%`;
}

const roots = collectRoots();
if (roots.length === 0) {
  console.error(
    'Nothing to parse: create corpus/ (see docs/05-transforms.md), set XXMI_PACKAGE_SRC, or pass paths.',
  );
  process.exit(2);
}
for (const root of roots) {
  if (!existsSync(root.dir)) {
    console.error(`${root.label}: ${root.dir} does not exist`);
    process.exit(2);
  }
}

const reports: RootReport[] = [];
for (const root of roots) reports.push(await checkRoot(root));

const baselineFile = readJson(BASELINE);
const baseline = baselineFile.parse as ParseBaseline | undefined;
const regressions = reports.filter((r) => {
  const base = baseline?.roots[r.label];
  return base !== undefined && r.failed > base.failed;
});
const lintBaseline = baselineFile.lint as LintBaseline | undefined;
/** `root: rule` pairs with more diagnostics than the baseline recorded. */
const lintRegressions = reports.flatMap((r) => {
  const base = lintBaseline?.roots[r.label];
  if (!base) return [];
  return Object.entries(r.lint)
    .filter(([id, count]) => count > (base[id] ?? 0))
    .map(([id, count]) => `${r.label}: ${id} ${base[id] ?? 0} → ${count}`);
});
const crashes = reports.filter((r) => r.crash !== undefined);

if (flags.has('--json')) {
  console.log(
    JSON.stringify(
      {
        version: 1,
        roots: reports,
        regressions: regressions.map((r) => r.label),
        lintRegressions,
      },
      null,
      2,
    ),
  );
} else {
  for (const report of reports) {
    console.log(`\n${report.label} (${report.dir})`);
    for (const failure of report.failures) {
      const more = failure.issues > 1 ? `  (+${failure.issues - 1} more)` : '';
      const first = failure.first;
      if (first)
        console.log(`  ${failure.path}:${first.line}:${first.character}  ${first.message}${more}`);
    }
    const base = baseline?.roots[report.label];
    const vsBase = base ? `  [baseline: ${base.failed}/${base.files}]` : '';
    console.log(
      `  ${report.files - report.failed}/${report.files} clean, ${report.failed} with parse errors ` +
        `(failure rate ${percent(report.failureRate)}), ${report.nonUtf8} not UTF-8${vsBase}`,
    );
  }
  for (const report of reports) {
    const base = lintBaseline?.roots[report.label];
    const ids = [...new Set([...Object.keys(report.lint), ...Object.keys(base ?? {})])].sort();
    if (report.crash) console.log(`\n${report.label}: LINT CRASHED\n${report.crash}`);
    else if (ids.length === 0) console.log(`\n${report.label}: no lint diagnostics`);
    else {
      console.log(`\n${report.label} lint (${report.lintErrors} errors):`);
      for (const id of ids) {
        const was = base ? `  [baseline ${base[id] ?? 0}]` : '';
        console.log(`  ${id}  ${report.lint[id] ?? 0}${was}`);
      }
    }
  }
  for (const r of regressions)
    console.log(`\nREGRESSION: ${r.label} has more parse failures than the baseline.`);
  for (const r of lintRegressions) console.log(`REGRESSION: ${r}`);
}

if (flags.has('--update-baseline')) {
  if (!existsSync(CORPUS)) {
    console.error('corpus/ does not exist; nowhere to write the baseline.');
    process.exit(2);
  }
  const grammar = readJson(GRAMMAR_SOURCE);
  const next: ParseBaseline = {
    updatedAt: new Date().toISOString(),
    grammarCommit: typeof grammar.commit === 'string' ? grammar.commit : null,
    roots: Object.fromEntries(
      reports.map((r) => [
        r.label,
        { files: r.files, failed: r.failed, failureRate: r.failureRate },
      ]),
    ),
  };
  const lint: LintBaseline = {
    updatedAt: next.updatedAt,
    roots: Object.fromEntries(reports.map((r) => [r.label, r.lint])),
  };
  writeFileSync(BASELINE, `${JSON.stringify({ ...baselineFile, parse: next, lint }, null, 2)}\n`);
  if (!flags.has('--json')) console.log(`\nBaseline written to ${display(BASELINE)}`);
}

if (crashes.length > 0) process.exit(3);
const strictFailed = flags.has('--strict') && reports.some((r) => r.failed > 0 || r.lintErrors > 0);
process.exit(regressions.length > 0 || lintRegressions.length > 0 || strictFailed ? 1 : 0);
