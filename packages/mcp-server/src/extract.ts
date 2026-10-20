/**
 * Unpacks a GitHub tarball into a directory, keeping only the text files worth browsing. The
 * filter is the safety net for a repository that suddenly ships gigabytes of textures.
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

/** Top-level folders of the DLL repo that hold its own code; third-party code is skipped. */
const DLL_DIRS = new Set(['directx11', 'd3dcompiler', 'injector', 'injectorlib', 'docs']);

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
  if (role !== 'dll') return true;
  return segments.length === 1 || DLL_DIRS.has((segments[0] ?? '').toLowerCase());
}

/** Extracts the tarball stream into `dest` (created if missing); returns the bytes written. */
export async function extractTarball(
  stream: Readable,
  dest: string,
  role: RepoRole,
): Promise<number> {
  await mkdir(dest, { recursive: true });
  let total = 0;
  await pipeline(
    stream,
    extract({
      cwd: dest,
      strip: 1,
      filter: (path, entry) => {
        const type = 'type' in entry ? entry.type : 'File';
        if (type === 'Directory') return true;
        if (type !== 'File') return false;
        const size = 'size' in entry ? entry.size : 0;
        const rel = path.split('/').slice(1).join('/');
        if (rel === '' || !keepPath(role, rel) || size > MAX_FILE_BYTES) return false;
        total += size;
        if (total > MAX_TOTAL_BYTES) throw new Error('tarball exceeds the size cap');
        return true;
      },
    }),
  );
  return total;
}
