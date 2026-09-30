#!/usr/bin/env node
/**
 * `xxmi`: see docs/04-cli.md. Exit codes: 0 ok, 1 lint errors (or --max-warnings exceeded),
 * 2 usage error, 3 internal error.
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import {
  ConfigError,
  RULES,
  SpecLookup,
  Workspace,
  buildSnapshot,
  displayPath,
  findPackageRoot,
  lintPaths,
  loadSpec,
  type LintResult,
} from '@xxmi-lang/core';

const USAGE = `Usage:
  xxmi lint [paths…] [--format text|json] [--rules XM201,XM202] [--max-warnings N]
  xxmi index <package dir> --out FILE [--name NAME]

Run \`xxmi <command> --help\` for details.`;

class UsageError extends Error {}

/** Diagnostics as `path:line:col  severity  XM201  message`, paths relative to the cwd. */
export function formatText(result: LintResult): string {
  const lines: string[] = [];
  for (const file of result.files) {
    for (const d of file.diagnostics) {
      const { line, character } = d.range.start;
      lines.push(
        `${displayPath(file.path)}:${line + 1}:${character + 1}  ${d.severity}  ${d.id}  ${d.message}`,
      );
    }
  }
  const s = result.summary;
  lines.push(
    `${s.files} file${s.files === 1 ? '' : 's'}: ${s.error} error${s.error === 1 ? '' : 's'}, ${s.warning} warning${s.warning === 1 ? '' : 's'}, ${s.info} info, ${s.hint} hint${s.hint === 1 ? '' : 's'}`,
  );
  return lines.join('\n');
}

/** The versioned JSON output (schema/lint-output.v1.json). Paths use `/`, relative to the cwd. */
export function formatJson(result: LintResult): string {
  return JSON.stringify(
    {
      version: 1,
      files: result.files.map((f) => ({
        path: displayPath(f.path),
        encoding: f.encoding,
        diagnostics: f.diagnostics.map((d) => ({
          id: d.id,
          severity: d.severity,
          message: d.message,
          range: d.range,
        })),
      })),
      summary: result.summary,
    },
    null,
    2,
  );
}

async function lint(args: string[]): Promise<number> {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      format: { type: 'string', default: 'text' },
      rules: { type: 'string' },
      'max-warnings': { type: 'string' },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.help) {
    console.log(`xxmi lint [paths…]  Lint .ini files and folders (default: .)

  --format text|json   output format (default text)
  --rules XM201,XM202  run only these rules
  --max-warnings N     exit 1 if there are more than N warnings

Rules:
${RULES.map((r) => `  ${r.id}  ${r.severity.padEnd(7)}  ${r.description}`).join('\n')}`);
    return 0;
  }
  if (values.format !== 'text' && values.format !== 'json')
    throw new UsageError(`unknown --format ${values.format}`);
  const rules = values.rules
    ?.split(',')
    .map((r) => r.trim().toUpperCase())
    .filter(Boolean);
  const known = new Set(RULES.map((r) => r.id));
  for (const r of rules ?? []) if (!known.has(r)) throw new UsageError(`unknown rule ${r}`);
  let maxWarnings: number | undefined;
  if (values['max-warnings'] !== undefined) {
    maxWarnings = Number(values['max-warnings']);
    if (!Number.isInteger(maxWarnings) || maxWarnings < 0)
      throw new UsageError('--max-warnings must be a non-negative integer');
  }
  const paths = positionals.length > 0 ? positionals : ['.'];
  for (const p of paths) {
    try {
      statSync(p);
    } catch {
      throw new UsageError(`no such file or directory: ${p}`);
    }
  }

  const result = await lintPaths(paths, rules ? { rules } : {});
  console.log(values.format === 'json' ? formatJson(result) : formatText(result));
  if (result.summary.error > 0) return 1;
  if (maxWarnings !== undefined && result.summary.warning > maxWarnings) return 1;
  return 0;
}

function git(dir: string, ...args: string[]): string {
  try {
    return execFileSync('git', ['-C', dir, ...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    return '';
  }
}

async function index(args: string[]): Promise<number> {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      out: { type: 'string' },
      name: { type: 'string' },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.help) {
    console.log(`xxmi index <package dir> --out FILE [--name NAME]

Builds a library symbol snapshot from a package (the folder holding d3dx.ini): the sections,
globals and namespaces of d3dx.ini and every file it includes.`);
    return 0;
  }
  const dir = positionals[0];
  if (!dir || !values.out) throw new UsageError('xxmi index needs a package dir and --out FILE');
  const root = findPackageRoot(resolve(dir));
  if (!root) throw new UsageError(`no d3dx.ini found at or above ${dir}`);
  const workspace = new Workspace({ lookup: new SpecLookup(loadSpec()) });
  await workspace.loadPackage(root);
  const repository = git(root, 'remote', 'get-url', 'origin').replace(/\.git$/, '');
  const snapshot = buildSnapshot(workspace, root, {
    name: values.name ?? 'package',
    repository,
    commit: git(root, 'rev-parse', 'HEAD'),
  });
  const out = resolve(values.out);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(snapshot, null, 2)}\n`);
  console.error(
    `Indexed ${workspace.files.size} files: ${snapshot.sections.length} sections, ${snapshot.variables.length} globals, ${snapshot.namespaces.length} namespaces → ${displayPath(out)}`,
  );
  return 0;
}

export async function main(argv: string[]): Promise<number> {
  const [command, ...rest] = argv;
  try {
    switch (command) {
      case 'lint':
        return await lint(rest);
      case 'index':
        return await index(rest);
      case undefined:
      case '--help':
      case '-h':
        console.log(USAGE);
        return command === undefined ? 2 : 0;
      default:
        throw new UsageError(`unknown command ${command}`);
    }
  } catch (error) {
    if (
      error instanceof UsageError ||
      error instanceof ConfigError ||
      (error instanceof TypeError &&
        'code' in error &&
        String(error.code).startsWith('ERR_PARSE_ARGS'))
    ) {
      console.error(`xxmi: ${error.message}\n\n${USAGE}`);
      return 2;
    }
    console.error(
      `xxmi: internal error (please report it): ${error instanceof Error ? (error.stack ?? error.message) : String(error)}`,
    );
    return 3;
  }
}

if (import.meta.main) {
  process.exitCode = await main(process.argv.slice(2));
}
