/**
 * Parses every .ini under corpus/ and under a game package checkout, then prints the parse-failure
 * rate and the first error per failing file.
 *
 *   XXMI_PACKAGE_SRC=../ZZMI-Package pnpm test:corpus [paths…] [--json] [--strict] [--update-baseline]
 *
 * Roots: corpus/ (if present), each path in XXMI_PACKAGE_SRC (separated like PATH), and any
 * paths given as arguments.
 *
 * --update-baseline  store the counts in corpus/.baseline.json under "parse"
 * --strict           exit 1 if any file fails to parse
 * --json             print a JSON report instead of text
 *
 * Exit codes: 0 ok, 1 more failures than the baseline (or any failure with --strict),
 * 2 usage error (no roots found).
 */
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, delimiter, join, relative, resolve } from 'node:path';
import { parseFile, type FileParseResult } from '../packages/core/src/index.ts';

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
  return {
    label: root.label,
    dir: display(root.dir),
    files: files.length,
    failed: failures.length,
    failureRate: files.length === 0 ? 0 : failures.length / files.length,
    nonUtf8,
    failures,
  };
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

if (flags.has('--json')) {
  console.log(
    JSON.stringify(
      { version: 1, roots: reports, regressions: regressions.map((r) => r.label) },
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
  for (const r of regressions)
    console.log(`\nREGRESSION: ${r.label} has more parse failures than the baseline.`);
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
  writeFileSync(BASELINE, `${JSON.stringify({ ...baselineFile, parse: next }, null, 2)}\n`);
  if (!flags.has('--json')) console.log(`\nBaseline written to ${display(BASELINE)}`);
}

const strictFailed = flags.has('--strict') && reports.some((r) => r.failed > 0);
process.exit(regressions.length > 0 || strictFailed ? 1 : 0);
