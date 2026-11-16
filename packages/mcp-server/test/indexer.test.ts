import { existsSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { Catalog } from '../src/catalog.ts';
import { Indexer, nextDelayMs } from '../src/indexer.ts';
import { Store } from '../src/store.ts';
import {
  bundledLookup,
  defaultRemotes,
  fakeSources,
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
  const sources = fakeSources(defaultRemotes());
  const indexer = new Indexer({
    store,
    catalog,
    sources,
    logger: silentLogger,
    repos: TEST_REPOS,
    lookup: bundledLookup(),
    limits: { releases: 3, prereleases: 2 },
    gitCacheMaxMb: 512,
    pollIntervalMin: 0,
  });
  return { store, catalog, sources, indexer, dataDir };
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

  it('is ready without the baseline, which stays unindexed while its remote fails', async () => {
    const { catalog, sources, indexer } = await setup();
    delete sources.remotes['migoto'];

    const result = await indexer.runOnce();

    expect(result.errors).toHaveLength(1);
    expect(catalog.ready).toBe(true);
    expect(catalog.isReady(catalog.repoByName('migoto'))).toBe(false);
  });

  it('is not ready while a non-baseline repo is missing', async () => {
    const { catalog, sources, indexer } = await setup();
    delete sources.remotes['zzmi'];

    await indexer.runOnce();

    expect(catalog.ready).toBe(false);
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
    const { sources, indexer } = await setup();
    await indexer.runOnce();
    const downloads = sources.materialized.length;

    const result = await indexer.runOnce();

    expect(sources.materialized).toHaveLength(downloads);
    expect(result.published).toEqual([]);
  });

  it('swaps to a new commit atomically and prunes the old one a pass later', async () => {
    const { store, catalog, sources, indexer } = await setup();
    await indexer.runOnce();
    const zzmi = catalog.repoByName('zzmi');
    const seenDuringDownload: string[] = [];
    const original = sources.materialize.bind(sources);
    sources.materialize = (repo, sha, dest) => {
      // Readers still see the old commit while the new one is being fetched.
      seenDuringDownload.push(catalog.resolve(zzmi).sha);
      return original(repo, sha, dest);
    };

    sources.remotes['zzmi'] = { head: { name: 'main', sha: SHA_B }, tags: [] };
    await indexer.runOnce();

    expect(seenDuringDownload).toEqual([SHA_A]);
    expect(catalog.resolve(zzmi).sha).toBe(SHA_B);
    expect(store.hasCheckout(zzmi, SHA_A)).toBe(true);

    await indexer.runOnce();
    expect(store.hasCheckout(zzmi, SHA_A)).toBe(false);
    expect(store.hasCheckout(zzmi, SHA_B)).toBe(true);
  });

  it('keeps serving the old commit when the download fails', async () => {
    const { catalog, sources, indexer } = await setup();
    await indexer.runOnce();
    const zzmi = catalog.repoByName('zzmi');

    sources.remotes['zzmi'] = { head: { name: 'main', sha: SHA_B }, tags: [] };
    sources.failMaterialize.add('zzmi');
    const result = await indexer.runOnce();

    expect(result.errors.join()).toMatch(/bad gateway/);
    expect(catalog.resolve(zzmi).sha).toBe(SHA_A);

    sources.failMaterialize.clear();
    await indexer.runOnce();
    expect(catalog.resolve(zzmi).sha).toBe(SHA_B);
  });

  it('persists state so a restart serves the volume without the network', async () => {
    const first = await setup();
    await first.indexer.runOnce();

    const second = await setup(first.dataDir);

    expect(second.catalog.ready).toBe(true);
    expect(second.catalog.resolve(second.catalog.repoByName('dll')).sha).toBe(SHA_A);
  });

  it('shares one pass between concurrent callers', async () => {
    const { sources, indexer } = await setup();
    const [a, b] = await Promise.all([indexer.runOnce(), indexer.runOnce()]);

    expect(a).toBe(b);
    expect(sources.materialized.filter((r) => r.startsWith('dll@'))).toHaveLength(1);
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

describe('indexer release lookups', () => {
  const tags = [
    { name: 'v1.0.0', sha: TAG_SHA },
    { name: 'v1.1.0-beta', sha: SHA_B },
  ];

  it('asks GitHub only when the tag set changes, and again when forced', async () => {
    const { sources, indexer } = await setup();
    sources.remotes['dll'] = { head: { name: 'master', sha: SHA_A }, tags };

    await indexer.runOnce();
    await indexer.runOnce();
    expect(sources.releaseCalls.filter((a) => a === 'dll')).toHaveLength(1);

    sources.remotes['dll'] = {
      head: { name: 'master', sha: SHA_A },
      tags: [...tags, { name: 'v1.2.0', sha: 'd'.repeat(40) }],
    };
    await indexer.runOnce();
    expect(sources.releaseCalls.filter((a) => a === 'dll')).toHaveLength(2);

    await indexer.refresh();
    expect(sources.releaseCalls.filter((a) => a === 'dll')).toHaveLength(3);
  });

  it('never looks up releases for the head-only baseline', async () => {
    const { sources, indexer } = await setup();
    await indexer.runOnce();

    expect(sources.releaseCalls).not.toContain('migoto');
    expect(sources.materialized.filter((m) => m.startsWith('migoto@'))).toHaveLength(1);
  });

  it('keeps indexing tags as plain tags when the lookup fails', async () => {
    const { catalog, sources, indexer } = await setup();
    sources.remotes['dll'] = { head: { name: 'master', sha: SHA_A }, tags };
    sources.releases = () => Promise.reject(new Error('rate limited'));

    const result = await indexer.runOnce();

    expect(result.errors).toEqual([]);
    expect(catalog.resolve(catalog.repoByName('dll'), 'v1.0.0').kind).toBe('tag');
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
