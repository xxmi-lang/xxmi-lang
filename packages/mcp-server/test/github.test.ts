import { describe, expect, it } from 'vitest';
import { createGitHubClient, GitHubError } from '../src/github.ts';
import { TRACKED_REPOS } from '../src/repos.ts';

const zzmi = TRACKED_REPOS[1] ?? TRACKED_REPOS[0];
if (!zzmi) throw new Error('no tracked repos');

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), { status: 200, ...init });
}

describe('GitHub client', () => {
  it('reads the default branch head and revalidates with the ETag', async () => {
    const seen: (string | null)[] = [];
    const fetchFn: typeof fetch = (input, init) => {
      const url = input instanceof Request ? input.url : String(input);
      const headers = new Headers(init?.headers);
      seen.push(headers.get('if-none-match'));
      if (url.endsWith('/ZZMI-Package'))
        return Promise.resolve(jsonResponse({ default_branch: 'main' }));
      if (headers.get('if-none-match') === '"v1"')
        return Promise.resolve(new Response(null, { status: 304 }));
      return Promise.resolve(jsonResponse({ sha: 'abc' }, { headers: { etag: '"v1"' } }));
    };
    const client = createGitHubClient({ fetch: fetchFn, token: 't' });

    const first = await client.head(zzmi);
    const second = await client.head(zzmi);

    expect(first).toEqual({ name: 'main', sha: 'abc' });
    expect(second).toEqual(first);
    expect(seen).toContain('"v1"');
  });

  it('maps tags to name and sha and skips the call for zero tags', async () => {
    let calls = 0;
    const fetchFn: typeof fetch = () => {
      calls++;
      return Promise.resolve(
        jsonResponse([
          { name: 'v1', commit: { sha: 's1' } },
          { name: 'v0', commit: { sha: 's0' } },
        ]),
      );
    };
    const client = createGitHubClient({ fetch: fetchFn });

    expect(await client.tags(zzmi, 1)).toEqual([{ name: 'v1', sha: 's1' }]);
    expect(await client.tags(zzmi, 0)).toEqual([]);
    expect(calls).toBe(1);
  });

  it('explains an exhausted API rate limit', async () => {
    const fetchFn: typeof fetch = () =>
      Promise.resolve(
        new Response('{}', { status: 403, headers: { 'x-ratelimit-remaining': '0' } }),
      );
    const client = createGitHubClient({ fetch: fetchFn });

    await expect(client.head(zzmi)).rejects.toThrow(GitHubError);
    await expect(client.head(zzmi)).rejects.toThrow(/GITHUB_TOKEN/);
  });
});
