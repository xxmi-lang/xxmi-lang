/**
 * Unpacks a tar stream (plain or gzip, node-tar detects which) into a directory, keeping only
 * the text files worth browsing. The filter is the safety net for a repository that suddenly
 * ships gigabytes of textures.
 */
import { mkdir } from 'node:fs/promises';
import { pipeline } from 'node:stream/promises';
import type { Readable } from 'node:stream';
import { extract } from 'tar';
import type { RepoRole } from './repos.ts';

const TEXT_EXTENSIONS = new Set([
  '.ini',
  '.md',
  '.txt',
  '.cpp',
  '.h',
  '.hpp',
  '.c',
  '.hlsl',
  '.hlsli',
  '.py',
  '.json',
  '.toml',
  '.j2',
  '.yml',
  '.yaml',
  '.sln',
  '.vcxproj',
  '.cs',
  '.bat',
]);

/** Top-level folders worth keeping per role; third-party code in the others is skipped. */
const ROLE_DIRS: Partial<Record<RepoRole, ReadonlySet<string>>> = {
  dll: new Set(['directx11', 'd3dcompiler', 'injector', 'injectorlib', 'docs']),
  baseline: new Set(['directx11']),
};

export const MAX_FILE_BYTES = 2_000_000;
export const MAX_TOTAL_BYTES = 120_000_000;

function extensionOf(path: string): string {
  const dot = path.lastIndexOf('.');
  return dot < 0 ? '' : path.slice(dot).toLowerCase();
}

/** Whether `relPath` (inside the repository, `/`-separated) should be kept. */
export function keepPath(role: RepoRole, relPath: string): boolean {
  if (!TEXT_EXTENSIONS.has(extensionOf(relPath))) return false;
  const segments = relPath.split('/');
  if (segments.some((s) => s === '..' || s === '.git')) return false;
  const dirs = ROLE_DIRS[role];
  if (!dirs) return true;
  return segments.length === 1 || dirs.has((segments[0] ?? '').toLowerCase());
}

/** Extracts the tar stream into `dest` (created if missing); returns the bytes written. */
export async function extractTar(stream: Readable, dest: string, role: RepoRole): Promise<number> {
  await mkdir(dest, { recursive: true });
  let total = 0;
  await pipeline(
    stream,
    extract({
      cwd: dest,
      filter: (path, entry) => {
        const type = 'type' in entry ? entry.type : 'File';
        if (type === 'Directory') return true;
        if (type !== 'File') return false;
        const size = 'size' in entry ? entry.size : 0;
        if (path === '' || !keepPath(role, path) || size > MAX_FILE_BYTES) return false;
        total += size;
        if (total > MAX_TOTAL_BYTES) throw new Error('archive exceeds the size cap');
        return true;
      },
    }),
  );
  return total;
}
