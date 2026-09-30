/**
 * Paths inside inis follow Windows rules whatever OS the tooling runs on: `\` and `/` are both
 * separators and lookup is case-insensitive. See docs/01-language-model.md "File paths".
 * Never hand an ini path straight to node:path.
 */
import { readdirSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

/** Splits an ini path into segments, dropping empty and `.` segments. */
export function iniPathSegments(iniPath: string): string[] {
  return iniPath.split(/[\\/]+/).filter((s) => s !== '' && s !== '.');
}

/** Joins with `\`, the separator the DLL uses when building namespaces and include paths. */
export function joinIniPath(...parts: string[]): string {
  return parts.flatMap(iniPathSegments).join('\\');
}

/** Case-insensitive path comparison key: `\`-separated and lower-cased. */
export function iniPathKey(iniPath: string): string {
  return joinIniPath(iniPath).toLowerCase();
}

/** A host path shown to users: relative to `base` when inside it, always with `/`. */
export function displayPath(path: string, base = process.cwd()): string {
  const rel = relative(base, path);
  const shown = rel && !rel.startsWith('..') ? rel : path;
  return shown.split(sep).join('/');
}

export interface ResolvedPath {
  /** Host path of the file or directory found. */
  path: string;
  /** The ini spelling differs in case from the file on disk (works in game, may confuse tools). */
  caseMismatch: boolean;
}

/**
 * Resolves an ini-relative path against a host directory the way Windows would: segment by
 * segment, matching names case-insensitively. `..` goes up. Directory listings are cached per
 * resolver, so one resolver should serve a single lint run.
 */
export class PathResolver {
  private readonly listings = new Map<string, string[] | null>();

  resolve(baseDir: string, iniPath: string): ResolvedPath | undefined {
    let current = baseDir;
    let caseMismatch = false;
    for (const segment of iniPathSegments(iniPath)) {
      if (segment === '..') {
        current = join(current, '..');
        continue;
      }
      const entries = this.list(current);
      if (!entries) return undefined;
      const exact = entries.find((e) => e === segment);
      const match = exact ?? entries.find((e) => e.toLowerCase() === segment.toLowerCase());
      if (match === undefined) return undefined;
      if (exact === undefined) caseMismatch = true;
      current = join(current, match);
    }
    return { path: current, caseMismatch };
  }

  isDirectory(path: string): boolean {
    try {
      return statSync(path).isDirectory();
    } catch {
      return false;
    }
  }

  /** Directory entries (names), or null if `dir` can't be read. */
  list(dir: string): string[] | null {
    let entries = this.listings.get(dir);
    if (entries === undefined) {
      try {
        entries = readdirSync(dir);
      } catch {
        entries = null;
      }
      this.listings.set(dir, entries);
    }
    return entries;
  }
}
