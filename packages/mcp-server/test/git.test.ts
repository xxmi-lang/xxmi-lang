import { existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { describe, expect, it } from 'vitest';
import { Catalog } from '../src/catalog.ts';
import { createGitSources, parseLsRemote } from '../src/git.ts';
import { Indexer } from '../src/indexer.ts';
import { Store } from '../src/store.ts';
import { createSourceClient, type SourceClient } from '../src/upstream.ts';
import { bundledLookup, silentLogger, tempDir } from './helpers.ts';
import { LocalUpstream } from './upstream-repo.ts';

const SHA_1 = '1'.repeat(40);
const SHA_2 = '2'.repeat(40);
const SHA_3 = '3'.repeat(40);

/** Relative path to content of every file under `dir`. */
function readTree(dir: string, base = dir): Record<string, string> {
  const files: Record<string, string> = {};
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) Object.assign(files, readTree(path, base));
    else files[relative(base, path).split(sep).join('/')] = readFileSync(path, 'utf8');
  }
  return files;
}

describe('parseLsRemote', () => {
  it('reads the default branch, head and peeled tags', () => {
    const refs = parseLsRemote(
      [
        'ref: refs/heads/dev\tHEAD',
        `${SHA_1}\tHEAD`,
        `${SHA_1}\trefs/heads/dev`,
        `${SHA_2}\trefs/heads/other`,
        `${SHA_3}\trefs/tags/v1`,
        `${SHA_2}\trefs/tags/v2`,
        `${SHA_1}\trefs/tags/v2^{}`,
      ].join('\n'),
    );

    expect(refs).toEqual({
      defaultBranch: 'dev',
      head: SHA_1,
      tags: [
        { name: 'v1', sha: SHA_3 },
        { name: 'v2', sha: SHA_1 },
      ],
    });
  });

  it('rejects an empty repository', () => {
    expect(() => parseLsRemote('')).toThrow(/default branch/);
  });
});

// Each test spawns dozens of git processes; slow CI machines need the headroom.
const GIT_TEST_TIMEOUT_MS = 60_000;

describe('git sources against a local repository', { timeout: GIT_TEST_TIMEOUT_MS }, () => {
  it('lists the default branch, head and tags, peeling annotated ones', async () => {
    const upstream = new LocalUpstream();
    const first = upstream.commit({ 'README.md': 'one' });
    upstream.tag('v1');
    const second = upstream.commit({ 'README.md': 'two' });
    upstream.tag('v2', { annotated: true });
    const git = createGitSources(tempDir());

    const refs = await git.listRefs(upstream.repo());

    expect(refs.defaultBranch).toBe('main');
    expect(refs.head).toBe(second);
    expect(refs.tags).toEqual([
      { name: 'v1', sha: first },
      { name: 'v2', sha: second },
    ]);
    expect(upstream.revParse('v2')).not.toBe(second);
  });

  it('fetches one commit shallowly and writes exactly its text files', async () => {
    const dataDir = tempDir();
    const upstream = new LocalUpstream();
    upstream.commit({ 'README.md': 'old', 'gone.txt': 'x' });
    const sha = upstream.commit({
      'README.md': 'new',
      'docs/notes.txt': 'notes',
      'bin/model.dds': 'binary',
    });
    const git = createGitSources(dataDir);
    const repo = upstream.repo();
    await git.listRefs(repo);
    const dest = join(tempDir(), 'out');

    await git.materialize(repo, sha, dest);

    expect(readTree(dest)).toEqual({
      'README.md': 'new',
      'gone.txt': 'x',
      'docs/notes.txt': 'notes',
    });
    expect(existsSync(join(dataDir, 'git', 'test', 'upstream.git', 'shallow'))).toBe(true);
    const info = await git.commitInfo(repo, sha);
    expect(info.subject).toBe('change');
    expect(Number.isNaN(Date.parse(info.date))).toBe(false);
  });

  it("fails with git's message for a commit the remote does not have", async () => {
    const upstream = new LocalUpstream();
    upstream.commit({ 'README.md': 'x' });
    const git = createGitSources(tempDir());

    await expect(git.materialize(upstream.repo(), SHA_1, join(tempDir(), 'out'))).rejects.toThrow(
      /git fetch failed/,
    );
  });

  it('drops the bare clone once it is over the size limit', async () => {
    const upstream = new LocalUpstream();
    const sha = upstream.commit({ 'README.md': 'x' });
    const git = createGitSources(tempDir());
    const repo = upstream.repo();
    await git.materialize(repo, sha, join(tempDir(), 'out'));

    expect(await git.trimCache(repo, 10_000_000)).toBe(false);
    expect(await git.trimCache(repo, 1)).toBe(true);
    await git.materialize(repo, sha, join(tempDir(), 'again'));
  });
});

/** A `git` stand-in (run through node, so it works on every OS) that records its calls. */
function wrapperScript(dir: string, failWhen: string): { script: string; log: string } {
  const script = join(dir, 'git-wrapper.mjs');
  const log = join(dir, 'calls.log');
  writeFileSync(
    script,
    `import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
const args = process.argv.slice(2);
appendFileSync(${JSON.stringify(log)}, args.join(' ') + '\\n');
if (${failWhen}) {
  console.error('refusing');
  process.exit(1);
}
process.exit(spawnSync('git', args, { stdio: 'inherit' }).status ?? 1);
`,
  );
  return { script, log };
}

const callsOf = (log: string): string[] => readFileSync(log, 'utf8').split('\n').filter(Boolean);

describe('git cache recovery', { timeout: GIT_TEST_TIMEOUT_MS }, () => {
  async function materialized() {
    const dataDir = tempDir();
    const upstream = new LocalUpstream();
    const sha = upstream.commit({ 'README.md': 'x' });
    const repo = upstream.repo();
    const git = createGitSources(dataDir);
    await git.listRefs(repo);
    await git.materialize(repo, sha, join(tempDir(), 'first'));
    const bare = join(dataDir, 'git', 'test', 'upstream.git');
    return { upstream, repo, git, sha, bare };
  }

  it('re-creates a cache that lost its HEAD', async () => {
    const { repo, git, sha, bare } = await materialized();
    rmSync(join(bare, 'HEAD'));
    const dest = join(tempDir(), 'again');

    await git.listRefs(repo);
    await git.materialize(repo, sha, dest);

    expect(readTree(dest)).toEqual({ 'README.md': 'x' });
  });

  it('re-creates a cache with a stale shallow.lock', async () => {
    const { upstream, repo, git, bare } = await materialized();
    const next = upstream.commit({ 'README.md': 'y' });
    writeFileSync(join(bare, 'shallow.lock'), '');
    const dest = join(tempDir(), 'again');

    await git.listRefs(repo);
    await git.materialize(repo, next, dest);

    expect(readTree(dest)).toEqual({ 'README.md': 'y' });
    expect(existsSync(join(bare, 'shallow.lock'))).toBe(false);
  });

  it('rebuilds at most once per pass when the remote is unreachable', async () => {
    const upstream = new LocalUpstream();
    const first = upstream.commit({ 'README.md': 'x' });
    const dir = tempDir();
    const { script, log } = wrapperScript(dir, "args.includes('fetch')");
    const git = createGitSources(tempDir(), [process.execPath, script]);
    const repo = upstream.repo();
    await git.listRefs(repo);

    await expect(git.materialize(repo, first, join(dir, 'a'))).rejects.toThrow(/git fetch failed/);
    await expect(git.materialize(repo, first, join(dir, 'b'))).rejects.toThrow(/git fetch failed/);

    // One creation, one rebuild; the second materialize finds the budget spent.
    expect(callsOf(log).filter((line) => line.startsWith('init'))).toHaveLength(2);
  });
});

describe('fetch by sha fallback', { timeout: GIT_TEST_TIMEOUT_MS }, () => {
  it('fetches the ref the sha was listed under when the sha itself is refused', async () => {
    const upstream = new LocalUpstream();
    const sha = upstream.commit({ 'README.md': 'x' });
    const dir = tempDir();
    const refused = "args.includes('fetch') && /^[0-9a-f]{40}$/.test(args.at(-1))";
    const { script, log } = wrapperScript(dir, refused);
    const git = createGitSources(tempDir(), [process.execPath, script]);
    const repo = upstream.repo();
    await git.listRefs(repo);
    const dest = join(dir, 'out');

    await git.materialize(repo, sha, dest);

    expect(readTree(dest)).toEqual({ 'README.md': 'x' });
    const fetched = callsOf(log).filter((line) => line.includes(' fetch '));
    expect(fetched.map((line) => line.split(' ').at(-1))).toEqual([sha, 'refs/heads/main']);
  });
});

describe('refs that look like options', { timeout: GIT_TEST_TIMEOUT_MS }, () => {
  it('lists and fetches a tag named -evil as plain data', async () => {
    const upstream = new LocalUpstream();
    const sha = upstream.commit({ 'README.md': 'x' });
    upstream.forceTag('-evil');
    const git = createGitSources(tempDir());
    const repo = upstream.repo();

    const refs = await git.listRefs(repo);
    const dest = join(tempDir(), 'out');
    await git.materialize(repo, sha, dest);

    expect(refs.tags).toEqual([{ name: '-evil', sha }]);
    expect(readTree(dest)).toEqual({ 'README.md': 'x' });
    expect((await git.commitInfo(repo, sha)).subject).toBe('change');
  });
});

describe('indexer over a local repository', { timeout: GIT_TEST_TIMEOUT_MS }, () => {
  async function setup() {
    const upstream = new LocalUpstream();
    const repo = upstream.repo();
    const dataDir = tempDir();
    const real = createSourceClient({
      dataDir,
      fetch: () => Promise.resolve(new Response('[]')),
    });
    const fetched: string[] = [];
    const sources: SourceClient = {
      ...real,
      materialize: (r, sha, dest) => {
        fetched.push(sha);
        return real.materialize(r, sha, dest);
      },
    };
    const store = new Store(dataDir);
    await store.init();
    const catalog = new Catalog(store, [repo]);
    const indexer = new Indexer({
      store,
      catalog,
      sources,
      logger: silentLogger,
      repos: [repo],
      lookup: bundledLookup(),
      limits: { releases: 3, prereleases: 2 },
      gitCacheMaxMb: 512,
      pollIntervalMin: 0,
    });
    return { upstream, repo, store, catalog, indexer, fetched };
  }

  it('indexes head and tags, and does no fetch when nothing changed', async () => {
    const { upstream, repo, catalog, indexer, fetched } = await setup();
    upstream.commit({ 'README.md': 'one' });
    upstream.tag('v1');
    const head = upstream.commit({ 'README.md': 'two' });

    const first = await indexer.runOnce();
    const fetchedAfterFirst = fetched.length;
    const second = await indexer.runOnce();

    expect(first.errors).toEqual([]);
    expect(catalog.resolve(repo).sha).toBe(head);
    expect(catalog.resolve(repo, 'v1').kind).toBe('tag');
    expect(catalog.resolve(repo).kind).toBe('head');
    expect(fetchedAfterFirst).toBe(2);
    expect(second.published).toEqual([]);
    expect(fetched).toHaveLength(fetchedAfterFirst);
  });

  it('picks up a new commit and a new tag', async () => {
    const { upstream, repo, catalog, indexer } = await setup();
    upstream.commit({ 'README.md': 'one' });
    await indexer.runOnce();

    const head = upstream.commit({ 'README.md': 'two' });
    upstream.tag('v2');
    await indexer.runOnce();

    expect(catalog.resolve(repo).sha).toBe(head);
    expect(catalog.resolve(repo, 'v2').sha).toBe(head);
    expect(catalog.repoState(repo)?.branch.subject).toBe('change');
  });

  it('re-resolves a force-moved tag', async () => {
    const { upstream, repo, catalog, indexer } = await setup();
    const first = upstream.commit({ 'README.md': 'one' });
    upstream.tag('v1');
    await indexer.runOnce();
    expect(catalog.resolve(repo, 'v1').sha).toBe(first);

    const second = upstream.commit({ 'README.md': 'two' });
    upstream.tag('v1', { rev: second });
    const result = await indexer.runOnce();

    expect(result.errors).toEqual([]);
    expect(catalog.resolve(repo, 'v1').sha).toBe(second);
  });

  it('prunes a deleted tag one pass after it disappears', async () => {
    const { upstream, repo, store, catalog, indexer } = await setup();
    const tagged = upstream.commit({ 'README.md': 'one' });
    upstream.tag('v1');
    upstream.commit({ 'README.md': 'two' });
    await indexer.runOnce();
    expect(store.hasCheckout(repo, tagged)).toBe(true);

    upstream.deleteTag('v1');
    await indexer.runOnce();
    expect(() => catalog.resolve(repo, 'v1')).toThrow(/not indexed/);
    expect(store.hasCheckout(repo, tagged)).toBe(true);

    await indexer.runOnce();
    expect(store.hasCheckout(repo, tagged)).toBe(false);
  });
});
