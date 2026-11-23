/**
 * Lints every .ini under corpus/ and under a game package checkout with the Rust `xxmi` binary
 * (XXMI_PATH, else rust/target/release/xxmi built on demand). A file fails to parse when it has an
 * XM001 diagnostic. Prints the parse-failure rate with the first error per failing file, and
 * diagnostic counts per rule compared against the baseline (a jump in one rule usually means a
 * false-positive regression).
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
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, delimiter, join, relative, resolve } from 'node:path';

const REPO = resolve(import.meta.dirname, '..');
const CORPUS = join(REPO, 'corpus');
const BASELINE = join(CORPUS, '.baseline.json');
const GRAMMAR_SOURCE = join(REPO, 'rust/crates/tree-sitter-migoto/SOURCE.md');
const MAX_OUTPUT_BYTES = 1 << 30;

interface Root {
  label: string;
  dir: string;
}

type Encoding = 'utf8' | 'utf8bom' | 'unknown';

/** The part of `xxmi lint --format json` (schema/lint-output.v1.json) this script reads. */
interface LintOutput {
  files: {
    path: string;
    encoding: Encoding;
    diagnostics: {
      id: string;
      message: string;
      range: { start: { line: number; character: number } };
    }[];
  }[];
  summary: { error: number };
}

interface FileReport {
  path: string;
  encoding: Encoding;
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

/** Paths as users see them: relative to the cwd, with `/`. */
function display(path: string): string {
  return relative(process.cwd(), path).replaceAll('\\', '/');
}

function xxmiBinary(): string {
  if (process.env.XXMI_PATH) return process.env.XXMI_PATH;
  execFileSync(
    'cargo',
    [
      'build',
      '--release',
      '--quiet',
      '--manifest-path',
      join(REPO, 'rust/Cargo.toml'),
      '-p',
      'xxmi',
    ],
    { stdio: 'inherit' },
  );
  const exe = process.platform === 'win32' ? '.exe' : '';
  return join(REPO, 'rust/target/release', `xxmi${exe}`);
}

/** Throws with the process's stderr unless it exited 0 or 1 (lint's "findings" code). */
function runLint(dir: string): LintOutput {
  const run = spawnSync(xxmi, ['lint', dir, '--format', 'json'], {
    encoding: 'utf8',
    maxBuffer: MAX_OUTPUT_BYTES,
  });
  if (run.error) throw run.error;
  if (run.status !== 0 && run.status !== 1) {
    throw new Error(`xxmi lint exited with ${run.status}\n${run.stderr}`);
  }
  return JSON.parse(run.stdout) as LintOutput;
}

function parseFailures(lint: LintOutput): FileReport[] {
  return lint.files.flatMap((file) => {
    const issues = file.diagnostics.filter((d) => d.id === 'XM001');
    const [first] = issues;
    if (!first) return [];
    return [
      {
        path: file.path,
        encoding: file.encoding,
        issues: issues.length,
        first: {
          line: first.range.start.line + 1,
          character: first.range.start.character + 1,
          message: first.message,
        },
      },
    ];
  });
}

function lintCounts(lint: LintOutput): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const file of lint.files) {
    for (const d of file.diagnostics) counts[d.id] = (counts[d.id] ?? 0) + 1;
  }
  return Object.fromEntries(Object.entries(counts).sort(([x], [y]) => (x < y ? -1 : 1)));
}

function checkRoot(root: Root): RootReport {
  const report: RootReport = {
    label: root.label,
    dir: display(root.dir),
    files: 0,
    failed: 0,
    failureRate: 0,
    nonUtf8: 0,
    failures: [],
    lint: {},
    lintErrors: 0,
  };
  try {
    const lint = runLint(root.dir);
    report.files = lint.files.length;
    report.failures = parseFailures(lint);
    report.failed = report.failures.length;
    report.failureRate = report.files === 0 ? 0 : report.failed / report.files;
    report.nonUtf8 = lint.files.filter((f) => f.encoding === 'unknown').length;
    report.lint = lintCounts(lint);
    report.lintErrors = lint.summary.error;
  } catch (error) {
    report.crash = error instanceof Error ? (error.stack ?? error.message) : String(error);
  }
  return report;
}

function readJson(path: string): Record<string, unknown> {
  return existsSync(path)
    ? (JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>)
    : {};
}

/** The fork commit SOURCE.md records, e.g. `e0df4ec`. */
function grammarCommit(): string | null {
  return readFileSync(GRAMMAR_SOURCE, 'utf8').match(/commit `([0-9a-f]+)`/)?.[1] ?? null;
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

const xxmi = xxmiBinary();
const reports = roots.map(checkRoot);

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
  const next: ParseBaseline = {
    updatedAt: new Date().toISOString(),
    grammarCommit: grammarCommit(),
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
