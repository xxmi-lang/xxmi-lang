/**
 * GitHub release metadata: the one thing `git` can't tell us (release vs pre-release).
 * The only GitHub API call the server makes, and only when a repo's tag set changed,
 * so the unauthenticated limit (60/hour per IP) is never in play.
 */
import { fullName, type TrackedRepo } from './repos.ts';
import type { Logger } from './logger.ts';
import { errorMessage } from './logger.ts';

export interface ReleaseInfo {
  tag: string;
  name: string | null;
  prerelease: boolean;
  draft: boolean;
  publishedAt: string | null;
}

export type ReleaseMeta = Omit<ReleaseInfo, 'tag'>;

/** What is persisted per repo in `state.json`. */
export interface ReleasesState {
  /** Tag names the `releases` map was last fetched for; empty until the first success. */
  tags: string[];
  synced: boolean;
  releases: Record<string, ReleaseMeta>;
  /** Epoch ms of the last failed lookup; cleared by a success. */
  failedAt?: number;
}

type FetchFn = typeof fetch;

const API = 'https://api.github.com';
const TIMEOUT_MS = 15_000;
const RETRY_AFTER_FAILURE_MS = 60 * 60_000;

interface ApiRelease {
  tag_name: string;
  name: string | null;
  prerelease: boolean;
  draft: boolean;
  published_at: string | null;
}

/** The newest 30 releases of `repo`; `token` is sent only when configured. */
export async function fetchReleases(
  repo: TrackedRepo,
  options: { token?: string | undefined; fetch?: FetchFn } = {},
): Promise<ReleaseInfo[]> {
  const doFetch = options.fetch ?? fetch;
  const url = `${API}/repos/${fullName(repo)}/releases?per_page=30`;
  const res = await doFetch(url, {
    headers: {
      'user-agent': 'xxmi-mcp',
      accept: 'application/vnd.github+json',
      ...(options.token ? { authorization: `Bearer ${options.token}` } : {}),
    },
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  if (!res.ok) {
    const limited = res.headers.get('x-ratelimit-remaining') === '0';
    throw new Error(`GitHub ${res.status} for ${url}${limited ? ' (API rate limit reached)' : ''}`);
  }

  const list: unknown = await res.json();
  if (!Array.isArray(list)) throw new Error(`unexpected releases response (HTTP ${res.status})`);
  return (list as ApiRelease[]).map((r) => ({
    tag: r.tag_name,
    name: r.name,
    prerelease: r.prerelease,
    draft: r.draft,
    publishedAt: r.published_at,
  }));
}

const sameSet = (a: readonly string[], b: readonly string[]): boolean =>
  a.length === b.length && b.every((t) => a.includes(t));

/** True while the stored release map doesn't cover the current tags (tags may be unclassified). */
export function isStale(state: ReleasesState | undefined, tags: readonly string[]): boolean {
  return !state?.synced || !sameSet(state.tags, tags);
}

function shouldLookUp(
  state: ReleasesState | undefined,
  tags: readonly string[],
  now: number,
  force: boolean,
): boolean {
  if (!isStale(state, tags)) return force;
  if (force || state?.failedAt === undefined) return true;
  return now - state.failedAt >= RETRY_AFTER_FAILURE_MS;
}

export interface ReleaseLookup {
  releases(repo: TrackedRepo): Promise<ReleaseInfo[]>;
}

/**
 * The release state to use for `tags`: the stored one while the tag set is unchanged, else a
 * fresh lookup. A failed lookup keeps the previous map and is retried at most hourly.
 */
export async function refreshReleases(
  lookup: ReleaseLookup,
  repo: TrackedRepo,
  tags: readonly string[],
  previous: ReleasesState | undefined,
  o: { logger: Logger; now: number; force: boolean },
): Promise<ReleasesState> {
  if (previous && !shouldLookUp(previous, tags, o.now, o.force)) return previous;

  try {
    const infos = await lookup.releases(repo);
    return {
      tags: [...tags],
      synced: true,
      releases: Object.fromEntries(infos.map(({ tag, ...meta }) => [tag, meta])),
    };
  } catch (error) {
    // Log the first failure of a streak only: the hourly retries would repeat it.
    if (previous?.failedAt === undefined) {
      o.logger.warn('release lookup failed; new tags stay unclassified', {
        repo: fullName(repo),
        error: errorMessage(error),
      });
    }
    return {
      tags: previous?.tags ?? [],
      synced: previous?.synced ?? false,
      releases: previous?.releases ?? {},
      failedAt: o.now,
    };
  }
}
