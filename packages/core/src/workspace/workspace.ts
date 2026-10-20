/**
 * Workspace index (docs/01-language-model.md §4): loads files with the namespaces the DLL would
 * give them, follows `[Include]` like `ParseIncludedIniFiles`, and builds the symbol table.
 */
import { realpathSync } from 'node:fs';
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

/**
 * An `exclude_recursive` glob as `glob_to_regex` compiles it (PCRE2 glob, caseless), matched
 * against a single file or folder name.
 */
export function globToRegExp(glob: string): RegExp {
  let out = '';
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i] ?? '';
    if (c === '*') out += '.*';
    else if (c === '?') out += '.';
    else if (c === '[') {
      const end = glob.indexOf(']', i + 2);
      if (end === -1) out += '\\[';
      else {
        const body = glob
          .slice(i + 1, end)
          .replace(/^!/, '^')
          .replace(/\\/g, '\\\\');
        out += `[${body}]`;
        i = end;
      }
    } else out += c.replace(/[.+^${}()|\\\]]/g, '\\$&');
  }
  return new RegExp(`^${out}$`, 'is');
}

/** Used for files outside any package, where no d3dx.ini says what to skip. */
const DEFAULT_EXCLUDES = [globToRegExp('DISABLED*')];

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
  /** `exclude_recursive` globs from each loaded package's d3dx.ini `[Include]`. */
  private readonly excludes = new Map<string, RegExp[]>();
  /** Files reached through `include =` (or d3dx.ini itself), which exclusions don't apply to. */
  private readonly included = new Set<string>();
  private symbolTable: SymbolTable | undefined;
  /** Text of files open in an editor; wins over the file on disk. */
  private readonly overlays = new Map<string, string>();
  private readonly realPaths = new Map<string, string>();
  /** How each file was loaded, so it can be re-lowered with the same namespace. */
  private readonly origins = new Map<
    string,
    { namespaceRoot: string; packageRoot: string | undefined; namespace?: string }
  >();

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
    this.included.add(main.path);
    this.excludes.set(
      root,
      main.sections
        .filter((s) => s.name.text.toLowerCase() === 'include')
        .flatMap((s) => s.lines)
        .filter((l) => l.key?.text.toLowerCase() === 'exclude_recursive' && l.value?.text)
        .map((l) => globToRegExp(l.value?.text ?? '')),
    );
    await this.followIncludes(main, root, root);
  }

  /**
   * Whether the DLL skips `path` while walking `include_recursive` folders: a folder or file
   * name between `base` (default: the package root) and the file matches the package's
   * `exclude_recursive` globs (`DISABLED*` outside a package). Files reached through
   * `include =` are never excluded. Loads the package if needed.
   */
  async excluded(path: string, base?: string): Promise<boolean> {
    const full = resolve(path);
    const root = this.packageRoots.get(full) ?? findPackageRoot(dirname(full), this.resolver);
    if (root) await this.loadPackage(root);
    return this.isExcluded(full, base);
  }

  /** {@link excluded} for a loaded file (or one whose package is loaded). */
  isExcluded(path: string, base?: string): boolean {
    const full = resolve(path);
    if (this.included.has(full)) return false;
    const root = this.packageRoots.get(full) ?? findPackageRoot(dirname(full), this.resolver);
    const from = base ?? root ?? this.origins.get(full)?.namespaceRoot;
    if (!from) return false;
    const rel = relative(from, full);
    if (rel.startsWith('..') || rel === '') return false;
    const patterns = (root && this.excludes.get(root)) || DEFAULT_EXCLUDES;
    return rel.split(sep).some((name) => patterns.some((p) => p.test(name)));
  }

  /**
   * Opens (or updates) a file from an editor buffer: the text is used instead of the disk
   * contents until `closeDocument`. Re-lowers the file and follows any new includes.
   */
  async openDocument(path: string, text: string, fallbackRoot: string): Promise<IniFile> {
    const full = resolve(path);
    this.overlays.set(full, text);
    for (const alias of this.aliases(full)) {
      this.overlays.set(alias, text);
      await this.reload(alias);
    }
    if (this.files.has(full)) {
      const file = await this.reload(full);
      this.targets.add(full);
      if (file) return file;
    }
    return this.addTarget(full, fallbackRoot);
  }

  /**
   * Registers buffer text without loading the file, so files given together can include each
   * other before each is opened.
   */
  stageDocument(path: string, text: string): void {
    this.overlays.set(resolve(path), text);
  }

  /** Drops the editor buffer; the file is re-read from disk (or removed if it's gone). */
  async closeDocument(path: string): Promise<void> {
    const full = resolve(path);
    for (const p of [full, ...this.aliases(full)]) {
      this.overlays.delete(p);
      await this.reload(p);
    }
  }

  /**
   * Other loaded paths of the same file on disk (symlinks: a library linked into Core/ while
   * its working copy lives elsewhere). They follow the editor buffer of any of them.
   */
  aliases(path: string): string[] {
    const real = this.realPath(resolve(path));
    return [...this.files.keys()].filter((p) => p !== path && this.realPath(p) === real);
  }

  private realPath(path: string): string {
    let real = this.realPaths.get(path);
    if (real === undefined) {
      try {
        real = realpathSync(path);
      } catch {
        real = path;
      }
      this.realPaths.set(path, real);
    }
    return real;
  }

  /**
   * Re-lowers a loaded file from its overlay or from disk (after a file-watcher event). A file
   * that no longer exists is removed. Returns the new model, or undefined if removed/not loaded.
   */
  async reload(path: string): Promise<IniFile | undefined> {
    const full = resolve(path);
    const origin = this.origins.get(full);
    if (!origin) return undefined;
    this.files.delete(full);
    this.symbolTable = undefined;
    let file: IniFile;
    try {
      file = await this.load(full, origin.namespaceRoot, origin.packageRoot, origin.namespace);
    } catch {
      this.origins.delete(full);
      this.targets.delete(full);
      this.packageRoots.delete(full);
      return undefined;
    }
    await this.followIncludes(file, origin.namespaceRoot, origin.packageRoot);
    return file;
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
    const real = this.realPath(path);
    const overlay =
      this.overlays.get(path) ??
      [...this.overlays].find(([other]) => this.realPath(other) === real)?.[1];
    const { text, encoding } =
      overlay !== undefined
        ? { text: overlay, encoding: 'utf8' as const }
        : decodeText(await readFile(path));
    const file = await lowerIni({
      path,
      text,
      encoding,
      defaultNamespace: namespace ?? pathNamespace(path, namespaceRoot),
      lookup: this.lookup,
    });
    this.files.set(path, file);
    this.origins.set(
      path,
      namespace === undefined
        ? { namespaceRoot, packageRoot }
        : { namespaceRoot, packageRoot, namespace },
    );
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
        this.included.add(found.path);
        if (this.files.has(found.path)) continue;
        const included = await this.load(found.path, namespaceRoot, packageRoot);
        await this.followIncludes(included, namespaceRoot, packageRoot);
      }
    }
  }
}
