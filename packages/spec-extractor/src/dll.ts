import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { AnchorNotFoundError, CppFile } from './cpp.ts';

const COMPONENT_DIRS = ['D3DCompiler', 'Injector'];

/** The DirectX11 sources of a 3DMigoto-family DLL checkout. */
export class DllSource {
  private readonly files = new Map<string, CppFile>();
  /** Component folders (`D3DCompiler`, `Injector`) the checkout doesn't have. */
  readonly missingComponents: string[];

  constructor(files: Iterable<CppFile>, missingComponents: string[] = []) {
    for (const file of files) this.files.set(file.name, file);
    this.missingComponents = missingComponents;
  }

  /** Loads `<root>/DirectX11/*.{cpp,h}` plus the shared `<root>/util.h` (DXGI format names). */
  static load(root: string): DllSource {
    const dir = join(root, 'DirectX11');
    const names = readdirSync(dir)
      .filter((n) => /\.(cpp|h)$/.test(n))
      .sort();
    const files = names.map((n) => new CppFile(n, readFileSync(join(dir, n), 'utf8')));
    const util = join(root, 'util.h');
    if (existsSync(util)) files.push(new CppFile('util.h', readFileSync(util, 'utf8')));
    // Other binaries built from the same repo read d3dx.ini too: the D3DCompiler wrapper and the
    // Injector (3DMigoto Loader). Only their literal-section reads are used.
    const missing: string[] = [];
    for (const component of COMPONENT_DIRS) {
      const componentDir = join(root, component);
      if (!existsSync(componentDir)) {
        missing.push(component);
        continue;
      }
      for (const name of readdirSync(componentDir)
        .filter((n) => n.endsWith('.cpp'))
        .sort()) {
        files.push(
          new CppFile(`${component}/${name}`, readFileSync(join(componentDir, name), 'utf8')),
        );
      }
    }
    return new DllSource(files, missing);
  }

  file(name: string): CppFile {
    const file = this.files.get(name);
    if (!file) throw new AnchorNotFoundError(name, 'file');
    return file;
  }

  /** All files in name order, for scans that aren't tied to one symbol. */
  all(): CppFile[] {
    return [...this.files.values()].sort((a, b) => (a.name < b.name ? -1 : 1));
  }
}

/**
 * Runs extraction steps. In strict mode (the XXMI DLL) a missing anchor aborts extraction, so
 * source drift is noticed instead of silently shrinking the spec. In lenient mode (the vanilla
 * baseline, which lacks XXMI-only tables) a missing anchor is recorded and the step yields its
 * fallback.
 */
export class Extraction {
  readonly warnings: string[] = [];
  readonly dll: DllSource;
  readonly strict: boolean;

  constructor(dll: DllSource, strict: boolean) {
    this.dll = dll;
    this.strict = strict;
  }

  step<T>(fallback: T, run: () => T): T {
    try {
      return run();
    } catch (error) {
      if (this.strict || !(error instanceof AnchorNotFoundError)) throw error;
      this.warnings.push(error.message);
      return fallback;
    }
  }
}

export interface GitInfo {
  repository: string;
  commit: string;
  commitDate: string;
}

export function gitInfo(root: string): GitInfo {
  const git = (...args: string[]): string =>
    execFileSync('git', ['-C', root, ...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  const [commit = '', commitDate = ''] = git('log', '-1', '--format=%H%n%cI').split('\n');
  let repository = '';
  try {
    repository = git('remote', 'get-url', 'origin')
      .replace(/\.git$/, '')
      .replace(/^git@github\.com:/, 'https://github.com/');
  } catch {
    // A checkout without an origin remote (e.g. a local copy) has no repository URL.
  }
  return { repository, commit, commitDate };
}
