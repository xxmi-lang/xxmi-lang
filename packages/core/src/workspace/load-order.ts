/**
 * The order the DLL loads files in (`ParseIniFilesRecursive`, IniHandler.cpp): within a folder,
 * `.ini` files come before subfolders, and names sort with `_wcsicmp`.
 */
import { relative } from 'node:path';

/** Orders two paths relative to the same root; the lesser one is parsed first. */
export function compareLoadOrder(a: string, b: string): number {
  const left = a.split(/[\\/]/);
  const right = b.split(/[\\/]/);
  const shared = Math.min(left.length, right.length);

  for (let i = 0; i < shared; i++) {
    const leftIsFile = i === left.length - 1;
    const rightIsFile = i === right.length - 1;
    if (leftIsFile !== rightIsFile) return leftIsFile ? -1 : 1;

    // `_wcsicmp` lower-cases each UTF-16 unit, then compares the units.
    const x = (left[i] ?? '').toLowerCase();
    const y = (right[i] ?? '').toLowerCase();
    if (x !== y) return x < y ? -1 : 1;
  }
  return left.length - right.length;
}

/** `path` relative to `root`, or unchanged when it is outside it or there is no root. */
export function relativeToRoot(path: string, root: string | undefined): string {
  if (root === undefined) return path;
  const inside = relative(root, path);
  return inside.startsWith('..') ? path : inside;
}
