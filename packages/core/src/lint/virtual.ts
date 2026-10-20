/**
 * A path resolver over a fixed set of in-memory files, so `lintTexts` can resolve `include =`
 * and path checks without ever touching the host file system.
 */
import { dirname, join } from 'node:path';
import { PathResolver } from '../paths.ts';

export class VirtualPathResolver extends PathResolver {
  private readonly dirs = new Map<string, Set<string>>();

  /** `paths` are absolute host-style paths of the virtual files. */
  constructor(paths: Iterable<string>) {
    super();
    for (const path of paths) {
      for (let child = path, parent = dirname(child); ; child = parent, parent = dirname(child)) {
        const names = this.dirs.get(parent) ?? new Set<string>();
        names.add(child.slice(parent.length).replace(/^[\\/]+/, ''));
        this.dirs.set(parent, names);
        if (parent === dirname(parent)) break;
      }
    }
  }

  override isDirectory(path: string): boolean {
    return this.dirs.has(join(path));
  }

  override list(dir: string): string[] | null {
    const names = this.dirs.get(join(dir));
    return names ? [...names] : null;
  }
}
