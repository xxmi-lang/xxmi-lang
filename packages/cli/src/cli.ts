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
  decodeText,
  displayPath,
  bundledSnapshots,
  describeSnapshotSymbol,
  describeSymbol,
  findPackageRoot,
  formatLintText,
  listLibraries,
  listSpec,
  lookupSpec,
  renderSpecEntry,
  renderSymbol,
  type LoadedPackage,
  type SpecEntryKind,
  type SymbolSnapshot,
  lintResultDocument,
  lintText,
  lintPaths,
  relaunchWithWasmFlags,
  VERSION,
  defaultSpec,
  type LintOptions,
  type LintResult,
} from '@xxmi-lang/core';

const USAGE = `Usage:
  xxmi lint [paths…] [--format text|json] [--rules XM201,XM202] [--max-warnings N]
  xxmi lint --stdin --stdin-path <path> [--format text|json] [--rules …] [--max-warnings N]
  xxmi query <kind> [name] [--package DIR] [--json]
  xxmi index <package dir> --out FILE [--name NAME]
  xxmi lsp [--stdio]

Run \`xxmi <command> --help\` for details.`;

class UsageError extends Error {}

/** Diagnostics as `path:line:col  severity  XM201  message`, paths relative to the cwd. */
export function formatText(result: LintResult): string {
  return formatLintText(result, displayPath);
}

/** The versioned JSON output (schema/lint-output.v1.json). Paths use `/`, relative to the cwd. */
export function formatJson(result: LintResult): string {
  return JSON.stringify(lintResultDocument(result, displayPath), null, 2);
}

/** Everything of stdin as bytes. */
async function readAllStdin(): Promise<Uint8Array> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks);
}

/** Lints the bytes on stdin as if they were the file at `path` (`--stdin`), see `lintText`. */
async function lintStdin(
  path: string,
  bytes: Uint8Array,
  options: LintOptions,
): Promise<LintResult> {
  const { text, encoding } = decodeText(bytes);
  return lintText(path, text, options, encoding);
}

async function lint(args: string[], readStdin: () => Promise<Uint8Array>): Promise<number> {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      format: { type: 'string', default: 'text' },
      rules: { type: 'string' },
      'max-warnings': { type: 'string' },
      stdin: { type: 'boolean', default: false },
      'stdin-path': { type: 'string' },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.help) {
    console.log(`xxmi lint [paths…]  Lint .ini files and folders (default: .)
xxmi lint --stdin --stdin-path <path>  Lint the text on stdin as if it were the file <path>

  --format text|json   output format (default text)
  --rules XM201,XM202  run only these rules
  --max-warnings N     exit 1 if there are more than N warnings
  --stdin              read one file's text from stdin instead of from disk
  --stdin-path <path>  with --stdin: the file the text stands for. Its namespace and package come
                       from this path, which doesn't need to exist; paths in the output are this one

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
  const options: LintOptions = rules ? { rules } : {};
  let result: LintResult;
  if (values.stdin) {
    if (!values['stdin-path']) throw new UsageError('--stdin needs --stdin-path <path>');
    if (positionals.length > 0) throw new UsageError('--stdin lints stdin only; drop the paths');
    result = await lintStdin(values['stdin-path'], await readStdin(), options);
  } else {
    if (values['stdin-path'] !== undefined) throw new UsageError('--stdin-path needs --stdin');
    const paths = positionals.length > 0 ? positionals : ['.'];
    for (const p of paths) {
      try {
        statSync(p);
      } catch {
        throw new UsageError(`no such file or directory: ${p}`);
      }
    }
    result = await lintPaths(paths, options);
  }
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
  const workspace = new Workspace({ lookup: new SpecLookup(defaultSpec()) });
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

/** Library data for `query symbol|library`: the package around `--package`/cwd, else the bundle. */
async function libraryData(
  dir: string | undefined,
): Promise<{ snapshot: SymbolSnapshot; pkg?: LoadedPackage }> {
  const root = findPackageRoot(resolve(dir ?? process.cwd()));
  if (root) {
    const workspace = new Workspace({ lookup: new SpecLookup(defaultSpec()) });
    await workspace.loadPackage(root);
    const snapshot = buildSnapshot(workspace, root, {
      name: 'package',
      repository: '',
      commit: '',
    });
    return { snapshot, pkg: { workspace, root } };
  }
  if (dir) throw new UsageError(`no d3dx.ini found at or above ${dir}`);
  const bundled = bundledSnapshots()[0];
  if (!bundled) throw new UsageError('no package found and no bundled library snapshot');
  return { snapshot: bundled };
}

const SPEC_QUERY_KINDS: Record<string, SpecEntryKind[]> = {
  command: ['command', 'function', 'member'],
  operator: ['operator'],
};

async function query(args: string[]): Promise<number> {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      json: { type: 'boolean', default: false },
      package: { type: 'string' },
      help: { type: 'boolean', default: false },
    },
  });
  if (values.help) {
    console.log(`xxmi query <kind> [name] [--package DIR] [--json]

Read-only lookups. Kinds:
  section-kinds          every section kind the DLL knows
  keys <section>         keys a section reads, e.g. Resource
  command <name>         a command, function or ->member: docs, args, DLL source
  operator <op>          an operator token with its precedence
  symbol <qualifiedName> a library symbol, e.g. CommandList\\ZZMI\\SetTextures
  library [name]         libraries (namespaces) of the package, with symbol counts

symbol and library read the package around --package (default: the current folder), else the
bundled ZZMI snapshot, which has locations but no doc comments. Not implemented yet: refs, outline.`);
    return 0;
  }
  const [kind, name] = positionals;
  const spec = defaultSpec();
  const print = (data: unknown, text: string): number => {
    console.log(values.json ? JSON.stringify(data, null, 2) : text);
    return 0;
  };
  const found = (entries: ReturnType<typeof lookupSpec>): number => {
    if (entries.length > 0)
      return print(entries, entries.map((e) => renderSpecEntry(e)).join('\n\n'));
    console.error(`xxmi: nothing found for ${kind ?? ''} ${name ?? ''}`);
    return 1;
  };

  switch (kind) {
    case 'section-kinds':
    case 'keys': {
      if (kind === 'keys' && !name) throw new UsageError('xxmi query keys needs a section name');
      const items = listSpec(spec, kind === 'keys' ? 'key' : 'section', name);
      return print(
        items,
        items
          .map(
            (i) => `${i.name}${i.xxmi ? ' [XXMI-only]' : ''}${i.summary ? ` - ${i.summary}` : ''}`,
          )
          .join('\n'),
      );
    }
    case 'command':
    case 'operator': {
      if (!name) throw new UsageError(`xxmi query ${kind} needs a name`);
      const kinds = SPEC_QUERY_KINDS[kind] ?? [];
      return found(kinds.flatMap((k) => lookupSpec(spec, { name, kind: k })));
    }
    case 'symbol': {
      if (!name) throw new UsageError('xxmi query symbol needs a qualified name');
      const { snapshot, pkg } = await libraryData(values.package);
      const info = pkg ? describeSymbol(pkg, name) : describeSnapshotSymbol(snapshot, name);
      if (!info) {
        console.error(`xxmi: no symbol ${name}`);
        return 1;
      }
      return print(info, renderSymbol(info));
    }
    case 'library': {
      const { snapshot, pkg } = await libraryData(values.package);
      const libs = listLibraries(snapshot, new SpecLookup(spec), pkg).filter(
        (l) => !name || l.namespace.toLowerCase() === name.toLowerCase(),
      );
      if (name && libs.length === 0) {
        console.error(`xxmi: no library ${name}`);
        return 1;
      }
      const counts = (l: (typeof libs)[number]): string =>
        Object.entries(l.counts)
          .map(([k, n]) => `${n} ${k}`)
          .join(', ');
      return print(
        libs,
        libs
          .map(
            (l) =>
              `${l.namespace}  (${counts(l)})${l.moduleDoc ? `\n  ${l.moduleDoc.text.split('\n')[0] ?? ''}` : ''}`,
          )
          .join('\n'),
      );
    }
    case 'refs':
    case 'outline':
      throw new UsageError(`xxmi query ${kind} is not implemented yet`);
    default:
      throw new UsageError(`unknown query kind ${kind ?? '(none)'}; see xxmi query --help`);
  }
}

/** `xxmi lsp`: the same language server as `xxmi-lsp`, so one binary can serve both. */
async function lsp(args: string[]): Promise<number> {
  if (args.includes('--help') || args.includes('-h')) {
    console.log(`xxmi lsp [--stdio]

Starts the language server for XXMI / 3DMigoto mod .ini files on stdin/stdout, exactly like
\`xxmi-lsp\`. stdio is the only transport, so --stdio is optional.`);
    return 0;
  }
  // Loaded on demand: the server (and vscode-languageserver) isn't needed by the other commands.
  const { main: startLsp } = await import('@xxmi-lang/lsp/main');
  startLsp(args);
  return 0; // the connection keeps the process alive until the client exits
}

/** `readStdin` is replaceable for tests. */
export async function main(
  argv: string[],
  readStdin: () => Promise<Uint8Array> = readAllStdin,
): Promise<number> {
  const [command, ...rest] = argv;
  try {
    switch (command) {
      case 'lint':
        return await lint(rest, readStdin);
      case 'index':
        return await index(rest);
      case 'query':
        return await query(rest);
      case 'lsp':
        return await lsp(rest);
      case '--version':
      case '-v':
        console.log(VERSION);
        return 0;
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

if (import.meta.main && !(await relaunchWithWasmFlags())) {
  process.exitCode = await main(process.argv.slice(2));
}
