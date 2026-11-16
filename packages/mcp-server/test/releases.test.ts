import { describe, expect, it } from 'vitest';
import {
  fetchReleases,
  refreshReleases,
  type ReleaseInfo,
  type ReleasesState,
} from '../src/releases.ts';
import { silentLogger } from '../src/logger.ts';
import { TRACKED_REPOS } from '../src/repos.ts';

const repo = TRACKED_REPOS[0];
if (!repo) throw new Error('no tracked repos');

const HOUR = 60 * 60_000;

const info = (tag: string, extra: Partial<ReleaseInfo> = {}): ReleaseInfo => ({
  tag,
  name: tag,
  prerelease: false,
  draft: false,
  publishedAt: '2026-01-01T00:00:00Z',
  ...extra,
});

function lookup(answers: (ReleaseInfo[] | Error)[]) {
  let calls = 0;
  return {
    get calls() {
      return calls;
    },
    releases(): Promise<ReleaseInfo[]> {
      const answer = answers[Math.min(calls++, answers.length - 1)];
      return answer instanceof Error ? Promise.reject(answer) : Promise.resolve(answer ?? []);
    },
  };
}

const options = (now: number, force = false) => ({ logger: silentLogger, now, force });

describe('fetchReleases', () => {
  const body = [
    {
      tag_name: 'v1.3.1',
      name: 'One three one',
      prerelease: true,
      draft: false,
      published_at: '2026-11-15T10:00:00Z',
    },
  ];

  it('maps the API answer and asks for 30 releases', async () => {
    const urls: string[] = [];
    const fetchFn: typeof fetch = (input) => {
      urls.push(input instanceof Request ? input.url : String(input));
      return Promise.resolve(new Response(JSON.stringify(body)));
    };

    const releases = await fetchReleases(repo, { fetch: fetchFn });

    expect(urls).toEqual([
      'https://api.github.com/repos/SpectrumQT/XXMI-Libs-Package/releases?per_page=30',
    ]);
    expect(releases).toEqual([
      {
        tag: 'v1.3.1',
        name: 'One three one',
        prerelease: true,
        draft: false,
        publishedAt: '2026-11-15T10:00:00Z',
      },
    ]);
  });

  it('sends the token only when configured', async () => {
    const seen: (string | null)[] = [];
    const fetchFn: typeof fetch = (_input, init) => {
      seen.push(new Headers(init?.headers).get('authorization'));
      return Promise.resolve(new Response('[]'));
    };

    await fetchReleases(repo, { fetch: fetchFn });
    await fetchReleases(repo, { fetch: fetchFn, token: 'secret' });

    expect(seen).toEqual([null, 'Bearer secret']);
  });

  it('explains an exhausted rate limit', async () => {
    const fetchFn: typeof fetch = () =>
      Promise.resolve(
        new Response('{}', { status: 403, headers: { 'x-ratelimit-remaining': '0' } }),
      );

    await expect(fetchReleases(repo, { fetch: fetchFn })).rejects.toThrow(/rate limit/);
  });

  it('rejects an answer that is not a list', async () => {
    const fetchFn: typeof fetch = () =>
      Promise.resolve(new Response('{"message":"nope"}', { status: 200 }));

    await expect(fetchReleases(repo, { fetch: fetchFn })).rejects.toThrow(
      /unexpected releases response \(HTTP 200\)/,
    );
  });
});

describe('refreshReleases', () => {
  it('looks up once at the first start and again only when the tag set changes', async () => {
    const source = lookup([[info('v1'), info('v2', { prerelease: true })]]);

    const first = await refreshReleases(source, repo, ['v1', 'v2'], undefined, options(0));
    const same = await refreshReleases(source, repo, ['v2', 'v1'], first, options(1));
    const changed = await refreshReleases(source, repo, ['v1', 'v2', 'v3'], same, options(2));

    expect(source.calls).toBe(2);
    expect(first.releases['v2']?.prerelease).toBe(true);
    expect(same).toBe(first);
    expect(changed.tags).toEqual(['v1', 'v2', 'v3']);
  });

  it('keeps the old map when the lookup fails and retries at most hourly', async () => {
    const source = lookup([[info('v1')], new Error('rate limited'), [info('v1'), info('v2')]]);
    const synced = await refreshReleases(source, repo, ['v1'], undefined, options(0));

    const failed = await refreshReleases(source, repo, ['v1', 'v2'], synced, options(10));
    const throttled = await refreshReleases(source, repo, ['v1', 'v2'], failed, options(HOUR - 1));
    const retried = await refreshReleases(
      source,
      repo,
      ['v1', 'v2'],
      throttled,
      options(HOUR + 10),
    );

    expect(failed.releases).toEqual(synced.releases);
    expect(failed.tags).toEqual(['v1']);
    expect(failed.failedAt).toBe(10);
    expect(throttled).toBe(failed);
    expect(source.calls).toBe(3);
    expect(retried.releases['v2']).toBeDefined();
    expect(retried.failedAt).toBeUndefined();
  });

  it('logs the first failure of a streak only', async () => {
    const warnings: string[] = [];
    const logger = { ...silentLogger, warn: (msg: string) => warnings.push(msg) };
    const source = lookup([new Error('down')]);

    const first = await refreshReleases(source, repo, ['v1'], undefined, {
      logger,
      now: 0,
      force: false,
    });
    await refreshReleases(source, repo, ['v1'], first, { logger, now: HOUR + 1, force: false });

    expect(first.synced).toBe(false);
    expect(source.calls).toBe(2);
    expect(warnings).toHaveLength(1);
  });

  it('looks up again when forced, even if nothing changed or a retry is throttled', async () => {
    const source = lookup([[info('v1')]]);
    const synced = await refreshReleases(source, repo, ['v1'], undefined, options(0));

    await refreshReleases(source, repo, ['v1'], synced, options(1, true));
    const failing: ReleasesState = { ...synced, tags: [], failedAt: 5 };
    await refreshReleases(source, repo, ['v1'], failing, options(6, true));

    expect(source.calls).toBe(3);
  });
});
