/**
 * Runs the rules over a workspace. The LSP, CLI and MCP all lint through here
 * (docs/02-diagnostics.md).
 */
import { readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import type { IniFile } from '../model/types.ts';
import type { TextEncoding } from '../parser/index.ts';
import { bundledSnapshots, defaultSpec } from '../defaults.ts';
import type { PathResolver } from '../paths.ts';
import { SpecLookup } from '../spec/lookup.ts';
import type { SymbolSnapshot } from '../workspace/symbols.ts';
import { Workspace } from '../workspace/workspace.ts';
import { ConfigLoader, type RuleSetting } from './config.ts';
import { XM101, XM102, XM103, XM104, XM105, XM106, XM107, XM108 } from './rules/structure.ts';
import { XM201, XM202, XM203, XM204, XM205, XM206, XM207, XM208 } from './rules/references.ts';
import { XM001, XM002, XM109 } from './rules/syntax.ts';
import { suppressionFilter } from './suppress.ts';
import type { Diagnostic, Rule, Severity } from './types.ts';

export const RULES: readonly Rule[] = [
  XM001,
  XM002,
  XM101,
  XM102,
  XM103,
  XM104,
  XM105,
  XM106,
  XM107,
  XM108,
  XM109,
  XM201,
  XM202,
  XM203,
  XM204,
  XM205,
  XM206,
  XM207,
  XM208,
];

export interface LintOptions {
  /** Only run these rule ids. */
  rules?: string[];
  lookup?: SpecLookup;
  snapshots?: SymbolSnapshot[];
  /** Replaces the host file system for include and path checks (see `lintTexts`). */
  resolver?: PathResolver;
  configs?: ConfigLoader;
  /** Rule settings that win over xxmi.toml (editor settings, docs/03-lsp.md). */
  overrides?: Record<string, RuleSetting>;
  /** Only run rules whose id starts with one of these (e.g. `['XM0', 'XM1']`). */
  rulePrefixes?: string[];
}

export interface FileResult {
  path: string;
  encoding: TextEncoding;
  diagnostics: Diagnostic[];
}

export interface LintResult {
  files: FileResult[];
  summary: Record<Severity, number> & { files: number };
}

/** Lints one loaded file against its workspace. */
export function lintFile(
  file: IniFile,
  workspace: Workspace,
  options: LintOptions = {},
): Diagnostic[] {
  const config = (options.configs ?? new ConfigLoader()).forDirectory(dirname(file.path));
  const only = options.rules ? new Set(options.rules.map((r) => r.toUpperCase())) : undefined;
  const diagnostics: Diagnostic[] = [];
  for (const rule of RULES) {
    if (only && !only.has(rule.id)) continue;
    if (options.rulePrefixes && !options.rulePrefixes.some((p) => rule.id.startsWith(p))) continue;
    const setting = options.overrides?.[rule.id] ?? config.rules[rule.id] ?? rule.severity;
    if (setting === 'off') continue;
    rule.check({
      file,
      workspace,
      symbols: workspace.symbols,
      lookup: workspace.lookup,
      resolver: workspace.resolver,
      packageRoot: workspace.packageRoot(file),
      report(range, message, fix) {
        diagnostics.push({
          id: rule.id,
          severity: setting,
          message,
          path: file.path,
          range,
          ...(fix ? { fix } : {}),
        });
      },
    });
  }
  const keep = suppressionFilter(file);
  return diagnostics
    .filter(keep)
    .sort(
      (a, b) =>
        a.range.start.line - b.range.start.line ||
        a.range.start.character - b.range.start.character ||
        a.id.localeCompare(b.id),
    );
}

/** `.ini` files under `path` (a file is returned as is), in a stable order. */
export function collectIniFiles(path: string): string[] {
  const full = resolve(path);
  if (!statSync(full).isDirectory()) return [full];
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : 1,
    )) {
      if (entry.name.startsWith('.') || entry.name === 'node_modules') continue;
      const child = join(dir, entry.name);
      if (entry.isDirectory()) walk(child);
      else if (entry.name.toLowerCase().endsWith('.ini')) out.push(child);
    }
  };
  walk(full);
  return out;
}

/** Lints files and folders; the CLI's `xxmi lint`. */
export async function lintPaths(paths: string[], options: LintOptions = {}): Promise<LintResult> {
  const lookup = options.lookup ?? new SpecLookup(defaultSpec());
  const workspace = newWorkspace(lookup, options);
  const targets: IniFile[] = [];
  for (const path of paths) {
    const full = resolve(path);
    const base = statSync(full).isDirectory() ? full : dirname(full);
    for (const file of collectIniFiles(full)) {
      // Like the DLL, skip what `exclude_recursive` skips (DISABLED folders) inside a folder.
      if (file !== full && (await workspace.excluded(file, full))) continue;
      targets.push(await workspace.addTarget(file, base));
    }
  }
  return lintTargets(targets, workspace, options);
}

/**
 * Lints `text` as if it were the file at `path` (the CLI's `--stdin`): the text goes into the workspace
 * the way the language server opens an editor buffer, so the namespace and the package come from `path`
 * (which doesn't have to exist). Like `lintPaths` on a file, only that file and its package's d3dx.ini
 * includes are loaded: sibling files of the mod aren't.
 */
export async function lintText(
  path: string,
  text: string,
  options: LintOptions = {},
  encoding?: TextEncoding,
): Promise<LintResult> {
  const lookup = options.lookup ?? new SpecLookup(defaultSpec());
  const workspace = newWorkspace(lookup, options);
  const full = resolve(path);
  const file = await workspace.openDocument(full, text, dirname(full));
  return lintTargets([encoding ? { ...file, encoding } : file], workspace, options);
}

/**
 * Lints several in-memory files together (the hosted MCP server's `lint_ini`): each is opened
 * like an editor buffer, so `include =` between them resolves. Pass `options.resolver` to keep
 * include and path checks off the host file system. A `d3dx.ini` opens first, since it decides
 * the package root of the rest.
 */
export async function lintTexts(
  files: { path: string; text: string }[],
  options: LintOptions = {},
): Promise<LintResult> {
  const lookup = options.lookup ?? new SpecLookup(defaultSpec());
  const workspace = newWorkspace(lookup, options);
  const ordered = [...files].sort((a, b) => Number(isD3dx(b.path)) - Number(isD3dx(a.path)));
  for (const { path, text } of ordered) workspace.stageDocument(path, text);
  const targets: IniFile[] = [];
  for (const { path, text } of ordered) {
    const full = resolve(path);
    targets.push(await workspace.openDocument(full, text, dirname(full)));
  }
  return lintTargets(targets, workspace, options);
}

const isD3dx = (path: string): boolean => /(^|[\\/])d3dx\.ini$/i.test(path);

function newWorkspace(lookup: SpecLookup, options: LintOptions): Workspace {
  return new Workspace({
    lookup,
    snapshots: options.snapshots ?? bundledSnapshots(),
    ...(options.resolver ? { resolver: options.resolver } : {}),
  });
}

function lintTargets(targets: IniFile[], workspace: Workspace, options: LintOptions): LintResult {
  const configs = options.configs ?? new ConfigLoader();
  const seen = new Set<string>();
  const files: FileResult[] = [];
  for (const file of targets) {
    if (seen.has(file.path)) continue;
    seen.add(file.path);
    files.push({
      path: file.path,
      encoding: file.encoding,
      diagnostics: lintFile(file, workspace, { ...options, configs }),
    });
  }
  const summary = { files: files.length, error: 0, warning: 0, info: 0, hint: 0 };
  for (const f of files) for (const d of f.diagnostics) summary[d.severity]++;
  return { files, summary };
}
