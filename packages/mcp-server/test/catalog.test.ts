import { describe, expect, it } from 'vitest';
import {
  Catalog,
  latestPrerelease,
  latestRelease,
  type RepoState,
  type VersionRef,
} from '../src/catalog.ts';
import { Store } from '../src/store.ts';
import { tempDir, TEST_REPOS } from './helpers.ts';

const version = (name: string, kind: VersionRef['kind'], publishedAt?: string): VersionRef => ({
  name,
  sha: name.replace(/\W/g, '').padEnd(40, 'f'),
  kind,
  ...(kind === 'head' ? {} : { tag: name }),
  ...(publishedAt ? { publishedAt } : {}),
});

async function catalogWith(tags: VersionRef[]) {
  const store = new Store(tempDir());
  await store.init();
  const catalog = new Catalog(store, TEST_REPOS);
  const repo = catalog.repoByName('dll');
  const entry: RepoState = {
    branch: version('master', 'head'),
    tags,
    indexedAt: '2026-11-16T00:00:00Z',
  };
  await catalog.publish({ version: 1, repos: { dll: entry }, releases: {} });
  return { catalog, repo, entry };
}

const withPrerelease = [
  version('v1.3.1', 'prerelease', '2026-11-15T00:00:00Z'),
  version('v1.2.3', 'release', '2026-10-31T00:00:00Z'),
  version('v1.2.2', 'release', '2026-09-01T00:00:00Z'),
];

describe('catalog ref resolution', () => {
  it('maps latest, head and an omitted ref to the default branch', async () => {
    const { catalog, repo } = await catalogWith(withPrerelease);

    for (const ref of [undefined, '', 'latest', 'head', 'HEAD']) {
      const resolved = catalog.resolve(repo, ref);
      expect(resolved.name).toBe('master');
      expect(resolved.isDefault).toBe(true);
      expect(resolved.kind).toBe('head');
    }
  });

  it('resolves latest-release and latest-prerelease', async () => {
    const { catalog, repo } = await catalogWith(withPrerelease);

    expect(catalog.resolve(repo, 'latest-release').name).toBe('v1.2.3');
    expect(catalog.resolve(repo, 'latest-prerelease').name).toBe('v1.3.1');
    expect(catalog.resolve(repo, 'latest-prerelease').kind).toBe('prerelease');
  });

  it('answers latest-prerelease with the release when no pre-release is newer', async () => {
    const { catalog, repo } = await catalogWith(withPrerelease.slice(1));

    expect(catalog.resolve(repo, 'latest-prerelease').name).toBe('v1.2.3');
  });

  it('resolves tag names and sha prefixes', async () => {
    const { catalog, repo, entry } = await catalogWith(withPrerelease);
    const second = entry.tags[1];

    expect(catalog.resolve(repo, 'V1.2.2').name).toBe('v1.2.2');
    expect(catalog.resolve(repo, second?.sha.slice(0, 8)).name).toBe('v1.2.3');
    expect(() => catalog.resolve(repo, 'v9')).toThrow(
      /Indexed: master, v1\.3\.1, v1\.2\.3, v1\.2\.2/,
    );
  });

  it('treats the newest tag as the latest release for repos without releases', async () => {
    const { catalog, repo, entry } = await catalogWith([
      version('v2', 'tag'),
      version('v1', 'tag'),
    ]);

    expect(catalog.resolve(repo, 'latest-release').name).toBe('v2');
    expect(latestRelease(entry)?.name).toBe('v2');
    expect(latestPrerelease(entry)).toBeUndefined();
  });

  it('has no latest-release when only pre-releases exist', async () => {
    const { catalog, repo } = await catalogWith([version('v1-beta', 'prerelease')]);

    expect(() => catalog.resolve(repo, 'latest-release')).toThrow(/not indexed/);
  });
});
