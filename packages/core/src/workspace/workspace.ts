/**
 * Workspace index (docs/01-language-model.md §4): loads files with the namespaces the DLL would
 * give them, follows `[Include]` like `ParseIncludedIniFiles`, and builds the symbol table.
 */
import { readFile } from 'node:fs/promises';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { decodeText } from '../parser/index.ts';
import { lowerIni } from '../model/lower.ts';
import type { IniFile } from '../model/types.ts';
import { PathResolver } from '../paths.ts';
import type { SpecLookup } from '../spec/lookup.ts';
import { SymbolTable, type SymbolSnapshot } from './symbols.ts';

export interface WorkspaceOptions {
  lookup: SpecLookup;
  resolver?: PathResolver;
  /** Library symbols to use when no package root provides them (the bundled ZZMI snapshot). */
  snapshots?: SymbolSnapshot[];
}

/** The folder holding d3dx.ini: the "3DMigoto folder" relative paths and namespaces start from. */
export function findPackageRoot(start: string, resolver = new PathResolver()): string | undefined {
  let dir = resolve(start);
  for (;;) {
    if (resolver.list(dir)?.some((name) => name.toLowerCase() === 'd3dx.ini')) return dir;
    const parent = dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

/** Namespace the DLL gives a file: its path relative to the 3DMigoto folder, `\`-separated. */
export function pathNamespace(path: string, root: string): string {
  return relative(root, path).split(sep).join('\\');
}

export class Workspace {
  readonly lookup: SpecLookup;
  readonly resolver: PathResolver;
  readonly files = new Map<string, IniFile>();
  /** Package root each file was loaded under, if any. */
  readonly packageRoots = new Map<string, string>();
  /** Files that were linted or explicitly added, as opposed to pulled in by includes. */
  readonly targets = new Set<string>();
  private readonly snapshots: SymbolSnapshot[];
  private readonly loadedPackages = new Set<string>();
  private symbolTable: SymbolTable | undefined;

  constructor(options: WorkspaceOptions) {
    this.lookup = options.lookup;
    this.resolver = options.resolver ?? new PathResolver();
    this.snapshots = options.snapshots ?? [];
  }

  /**
   * Adds a file to lint. Its namespace comes from its path relative to the package root (or to
   * `fallbackRoot` when there is none, which keeps namespaces unique within the run).
   */
  async addTarget(path: string, fallbackRoot: string): Promise<IniFile> {
    const full = resolve(path);
    const root = findPackageRoot(dirname(full), this.resolver);
    if (root) await this.loadPackage(root);
    const file = await this.load(full, root ?? fallbackRoot, root);
    this.targets.add(full);
    await this.followIncludes(file, root ?? fallbackRoot, root);
    return file;
  }

  /** Loads d3dx.ini and everything it `include`s (not `include_recursive` folders). */
  async loadPackage(root: string): Promise<void> {
    if (this.loadedPackages.has(root)) return;
    this.loadedPackages.add(root);
    const d3dx = this.resolver.list(root)?.find((n) => n.toLowerCase() === 'd3dx.ini');
    if (!d3dx) return;
    const main = await this.load(join(root, d3dx), root, root, '');
    await this.followIncludes(main, root, root);
  }

  get symbols(): SymbolTable {
    if (!this.symbolTable) {
      const table = new SymbolTable(this.lookup);
      for (const file of this.files.values()) table.addFile(file);
      for (const snapshot of this.snapshots) {
        if (!snapshot.namespaces.some((ns) => table.hasNamespace(ns))) table.addSnapshot(snapshot);
      }
      this.symbolTable = table;
    }
    return this.symbolTable;
  }

  packageRoot(file: IniFile): string | undefined {
    return this.packageRoots.get(file.path);
  }

  private async load(
    path: string,
    namespaceRoot: string,
    packageRoot: string | undefined,
    namespace?: string,
  ): Promise<IniFile> {
    const existing = this.files.get(path);
    if (existing) return existing;
    this.symbolTable = undefined;
    const { text, encoding } = decodeText(await readFile(path));
    const file = await lowerIni({
      path,
      text,
      encoding,
      defaultNamespace: namespace ?? pathNamespace(path, namespaceRoot),
      lookup: this.lookup,
    });
    this.files.set(path, file);
    if (packageRoot) this.packageRoots.set(path, packageRoot);
    return file;
  }

  /** `include = …` in `[Include…]` sections, relative to the including file's folder. */
  private async followIncludes(
    file: IniFile,
    namespaceRoot: string,
    packageRoot: string | undefined,
  ): Promise<void> {
    for (const section of file.sections) {
      if (section.spec?.name !== 'Include') continue;
      for (const line of section.lines) {
        if (line.key?.text.toLowerCase() !== 'include' || !line.value?.text) continue;
        const found = this.resolver.resolve(dirname(file.path), line.value.text);
        if (!found || this.resolver.isDirectory(found.path)) continue;
        if (this.files.has(found.path)) continue;
        const included = await this.load(found.path, namespaceRoot, packageRoot);
        await this.followIncludes(included, namespaceRoot, packageRoot);
      }
    }
  }
}
