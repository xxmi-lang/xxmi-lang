/**
 * Minimal GitHub client for the indexer: branch head, newest tags and source tarballs.
 * Unauthenticated by default (60 API requests/hour per IP); `GITHUB_TOKEN` raises that. Head
 * checks use ETags, because a 304 doesn't count against the limit, and tarballs come from
 * codeload, which isn't rate limited like the API.
 */
import { Readable } from 'node:stream';
import { fullName, type TrackedRepo } from './repos.ts';

export interface RemoteRef {
  name: string;
  sha: string;
}

export interface GitHubClient {
  /** The default branch and the commit it points at. */
  head(repo: TrackedRepo): Promise<RemoteRef>;
  /** The newest `count` tags, newest first. */
  tags(repo: TrackedRepo, count: number): Promise<RemoteRef[]>;
  /** The `.tar.gz` of the repository at `sha`. */
  tarball(repo: TrackedRepo, sha: string): Promise<Readable>;
}

export class GitHubError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'GitHubError';
    this.status = status;
  }
}

type FetchFn = typeof fetch;

interface Cached {
  etag: string;
  body: unknown;
}

const API = 'https://api.github.com';
const CODELOAD = 'https://codeload.github.com';
const TARBALL_TIMEOUT_MS = 180_000;
const API_TIMEOUT_MS = 15_000;

export function createGitHubClient(
  options: { token?: string | undefined; fetch?: FetchFn; userAgent?: string } = {},
): GitHubClient {
  const doFetch = options.fetch ?? fetch;
  const cache = new Map<string, Cached>();
  const headers = (extra: Record<string, string> = {}): Record<string, string> => ({
    'user-agent': options.userAgent ?? 'xxmi-mcp',
    accept: 'application/vnd.github+json',
    ...(options.token ? { authorization: `Bearer ${options.token}` } : {}),
    ...extra,
  });

  /** Default branch names change almost never: ask once per process and repo. */
  const branches = new Map<string, string>();

  async function getJson(url: string): Promise<unknown> {
    const cached = cache.get(url);
    const res = await doFetch(url, {
      headers: headers(cached ? { 'if-none-match': cached.etag } : {}),
      signal: AbortSignal.timeout(API_TIMEOUT_MS),
    });
    if (res.status === 304 && cached) return cached.body;
    if (!res.ok) {
      const limited = res.headers.get('x-ratelimit-remaining') === '0';
      throw new GitHubError(
        res.status,
        `GitHub ${res.status} for ${url}${limited ? ' (API rate limit reached; set GITHUB_TOKEN)' : ''}`,
      );
    }
    const body: unknown = await res.json();
    const etag = res.headers.get('etag');
    if (etag) cache.set(url, { etag, body });
    return body;
  }

  return {
    async head(repo) {
      const base = `${API}/repos/${fullName(repo)}`;
      let branch = branches.get(fullName(repo));
      if (!branch) {
        branch = ((await getJson(base)) as { default_branch: string }).default_branch;
        branches.set(fullName(repo), branch);
      }
      const commit = (await getJson(`${base}/commits/${encodeURIComponent(branch)}`)) as {
        sha: string;
      };
      return { name: branch, sha: commit.sha };
    },

    async tags(repo, count) {
      if (count <= 0) return [];
      const url = `${API}/repos/${fullName(repo)}/tags?per_page=${count}`;
      const list = (await getJson(url)) as { name: string; commit: { sha: string } }[];
      return list.slice(0, count).map((t) => ({ name: t.name, sha: t.commit.sha }));
    },

    async tarball(repo, sha) {
      const url = `${CODELOAD}/${fullName(repo)}/tar.gz/${sha}`;
      const res = await doFetch(url, {
        headers: headers({ accept: '*/*' }),
        signal: AbortSignal.timeout(TARBALL_TIMEOUT_MS),
      });
      if (!res.ok || !res.body)
        throw new GitHubError(res.status, `GitHub ${res.status} for ${url}`);
      return Readable.fromWeb(res.body);
    },
  };
}
