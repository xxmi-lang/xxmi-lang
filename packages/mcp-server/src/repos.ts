/**
 * The repositories the server tracks. The allowlist is code, not input: the server never
 * fetches a repository that isn't listed here (docs/06-mcp-server.md "Security and limits").
 */
export type RepoRole = 'dll' | 'package' | 'tools';

export interface TrackedRepo {
  /** Short name accepted by tools next to `owner/name`. */
  alias: string;
  owner: string;
  name: string;
  role: RepoRole;
  /** Package repos: the game profile this package belongs to. */
  game?: string;
  /** Package repos: the folder inside the repository that holds `d3dx.ini`. */
  packageDir?: string;
}

export const TRACKED_REPOS: readonly TrackedRepo[] = [
  { alias: 'dll', owner: 'SpectrumQT', name: 'XXMI-Libs-Package', role: 'dll' },
  {
    alias: 'zzmi',
    owner: 'leotorrez',
    name: 'ZZMI-Package',
    role: 'package',
    game: 'zzmi',
    packageDir: 'ZZMI',
  },
  { alias: 'xxmitools', owner: 'leotorrez', name: 'XXMITools', role: 'tools' },
];

export const fullName = (repo: TrackedRepo): string => `${repo.owner}/${repo.name}`;

/** Folder name under the data volume, safe on every file system. */
export const repoDirName = (repo: TrackedRepo): string => `${repo.owner}__${repo.name}`;

/** Finds a tracked repo by `owner/name` or alias, case-insensitively. */
export function findRepo(
  query: string,
  repos: readonly TrackedRepo[] = TRACKED_REPOS,
): TrackedRepo | undefined {
  const lower = query.trim().toLowerCase();
  return repos.find((r) => r.alias === lower || fullName(r).toLowerCase() === lower);
}

export function gameRepo(
  game: string,
  repos: readonly TrackedRepo[] = TRACKED_REPOS,
): TrackedRepo | undefined {
  return repos.find((r) => r.role === 'package' && r.game === game.trim().toLowerCase());
}

export function dllRepo(repos: readonly TrackedRepo[] = TRACKED_REPOS): TrackedRepo | undefined {
  return repos.find((r) => r.role === 'dll');
}
