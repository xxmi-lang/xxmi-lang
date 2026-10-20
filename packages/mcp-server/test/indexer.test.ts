import { existsSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { Catalog } from '../src/catalog.ts';
import { Indexer, nextDelayMs } from '../src/indexer.ts';
import { Store } from '../src/store.ts';
import {
  defaultRemotes,
  fakeGitHub,
  silentLogger,
  SHA_A,
  SHA_B,
  TAG_SHA,
  tempDir,
  TEST_REPOS,
} from './helpers.ts';

async function setup(dataDir = tempDir()) {
  const store = new Store(dataDir);
  await store.init();
  const catalog = new Catalog(store, TEST_REPOS);
  await catalog.load();
  const github = fakeGitHub(defaultRemotes());
  const indexer = new Indexer({
    store,
    catalog,
    github,
    logger: silentLogger,
    repos: TEST_REPOS,
    indexedTags: 5,
    pollIntervalMin: 0,
  });
  return { store, catalog, github, indexer, dataDir };
}

describe('indexer', () => {
  it('is not ready until the first pass publishes every default branch', async () => {
    const { catalog, indexer } = await setup();
    expect(catalog.ready).toBe(false);

    const result = await indexer.runOnce();

    expect(result.errors).toEqual([]);
    expect(catalog.ready).toBe(true);
    expect(catalog.resolve(catalog.repoByName('zzmi')).sha).toBe(SHA_A);
    expect(catalog.resolve(catalog.repoByName('zzmi'), 'v1.0.0').sha).toBe(TAG_SHA);
  });

  it('builds the package symbol index and keeps text sources only', async () => {
    const { store, catalog, indexer } = await setup();
    await indexer.runOnce();

    const zzmi = catalog.repoByName('zzmi');
    const index = await store.readIndex<{ sections: { qualifiedName: string }[] }>(zzmi, SHA_A);
    expect(index?.sections.map((s) => s.qualifiedName)).toContain('CommandList\\Demo\\SetThings');
    expect(existsSync(`${store.checkoutDir(zzmi, SHA_A)}/ZZMI/d3dx.ini`)).toBe(true);
  });

  it('downloads nothing when no commit changed', async () => {
    const { github, indexer } = await setup();
    await indexer.runOnce();
    const downloads = github.tarballRequests.length;

    const result = await indexer.runOnce();

    expect(github.tarballRequests).toHaveLength(downloads);
    expect(result.published).toEqual([]);
  });

  it('swaps to a new commit atomically and prunes the old one a pass later', async () => {
    const { store, catalog, github, indexer } = await setup();
    await indexer.runOnce();
    const zzmi = catalog.repoByName('zzmi');
    const seenDuringDownload: string[] = [];
    const original = github.tarball.bind(github);
    github.tarball = (repo, sha) => {
      // Readers still see the old commit while the new one is being fetched.
      seenDuringDownload.push(catalog.resolve(zzmi).sha);
      return original(repo, sha);
    };

    github.remotes['zzmi'] = { head: { name: 'main', sha: SHA_B }, tags: [] };
    await indexer.runOnce();

    expect(seenDuringDownload).toEqual([SHA_A]);
    expect(catalog.resolve(zzmi).sha).toBe(SHA_B);
    expect(store.hasCheckout(zzmi, SHA_A)).toBe(true);

    await indexer.runOnce();
    expect(store.hasCheckout(zzmi, SHA_A)).toBe(false);
    expect(store.hasCheckout(zzmi, SHA_B)).toBe(true);
  });

  it('keeps serving the old commit when the download fails', async () => {
    const { catalog, github, indexer } = await setup();
    await indexer.runOnce();
    const zzmi = catalog.repoByName('zzmi');

    github.remotes['zzmi'] = { head: { name: 'main', sha: SHA_B }, tags: [] };
    github.failTarballs.add('zzmi');
    const result = await indexer.runOnce();

    expect(result.errors.join()).toMatch(/bad gateway/);
    expect(catalog.resolve(zzmi).sha).toBe(SHA_A);

    github.failTarballs.clear();
    await indexer.runOnce();
    expect(catalog.resolve(zzmi).sha).toBe(SHA_B);
  });

  it('persists state so a restart serves the volume without GitHub', async () => {
    const first = await setup();
    await first.indexer.runOnce();

    const second = await setup(first.dataDir);

    expect(second.catalog.ready).toBe(true);
    expect(second.catalog.resolve(second.catalog.repoByName('dll')).sha).toBe(SHA_A);
  });

  it('shares one pass between concurrent callers', async () => {
    const { github, indexer } = await setup();
    const [a, b] = await Promise.all([indexer.runOnce(), indexer.runOnce()]);

    expect(a).toBe(b);
    expect(github.tarballRequests.filter((r) => r.startsWith('dll@'))).toHaveLength(1);
  });

  it('rejects refs that are not indexed and lists what is', async () => {
    const { catalog, indexer } = await setup();
    await indexer.runOnce();

    expect(() => catalog.resolve(catalog.repoByName('zzmi'), 'v9.9.9')).toThrow(
      /Indexed: main, v1\.0\.0/,
    );
    expect(() => catalog.repoByName('someone/else')).toThrow(/Tracked repos/);
  });
});

describe('nextDelayMs', () => {
  it('retries within a minute while the first index is missing, else waits the interval', () => {
    const failed = { published: [], errors: ['boom'] };
    const clean = { published: [], errors: [] };

    expect(nextDelayMs(failed, false, 15)).toBe(60_000);
    expect(nextDelayMs(failed, true, 15)).toBe(15 * 60_000);
    expect(nextDelayMs(clean, false, 15)).toBe(15 * 60_000);
  });
});
