import { describe, expect, it } from 'vitest';
import type { ReleaseMeta, ReleasesState } from '../src/releases.ts';
import { compareTagNames, selectVersions } from '../src/selection.ts';

const LIMITS = { releases: 3, prereleases: 2 };

const tag = (name: string) => ({ name, sha: name.padEnd(40, '0') });

const meta = (publishedAt: string | null, extra: Partial<ReleaseMeta> = {}): ReleaseMeta => ({
  name: null,
  prerelease: false,
  draft: false,
  publishedAt,
  ...extra,
});

const state = (releases: Record<string, ReleaseMeta>, tags: string[]): ReleasesState => ({
  tags,
  synced: true,
  releases,
});

const names = (versions: { name: string }[]): string[] => versions.map((v) => v.name);

describe('compareTagNames', () => {
  it('compares numbers as numbers', () => {
    expect(compareTagNames('v1.10.0', 'v1.9.0')).toBeGreaterThan(0);
    expect(compareTagNames('v1.2', 'v1.2')).toBe(0);
  });
});

describe('selectVersions', () => {
  const tags = ['v1.0', 'v1.1', 'v1.2', 'v1.3', 'v1.4-beta', 'v1.5-beta', 'v1.6-beta'].map(tag);
  const names0 = tags.map((t) => t.name);
  const releases = state(
    {
      'v1.0': meta('2026-01-01T00:00:00Z'),
      'v1.1': meta('2026-02-01T00:00:00Z'),
      'v1.2': meta('2026-03-01T00:00:00Z'),
      'v1.3': meta('2026-04-01T00:00:00Z'),
      'v1.4-beta': meta('2026-05-01T00:00:00Z', { prerelease: true }),
      'v1.5-beta': meta('2026-06-01T00:00:00Z', { prerelease: true }),
      'v1.6-beta': meta('2026-07-01T00:00:00Z', { prerelease: true }),
    },
    names0,
  );

  it('takes the newest releases and pre-releases within the limits', () => {
    const picked = selectVersions('dll', tags, releases, LIMITS);

    expect(names(picked)).toEqual(['v1.6-beta', 'v1.5-beta', 'v1.3', 'v1.2', 'v1.1']);
    expect(picked.map((v) => v.kind)).toEqual([
      'prerelease',
      'prerelease',
      'release',
      'release',
      'release',
    ]);
  });

  it('skips pre-releases older than the newest release and drafts', () => {
    const older = state(
      {
        'v1.0': meta('2026-01-01T00:00:00Z', { prerelease: true }),
        'v1.1': meta('2026-02-01T00:00:00Z', { draft: true }),
        'v1.2': meta('2026-03-01T00:00:00Z'),
      },
      ['v1.0', 'v1.1', 'v1.2'],
    );

    const picked = selectVersions('dll', ['v1.0', 'v1.1', 'v1.2'].map(tag), older, LIMITS);

    expect(names(picked)).toEqual(['v1.2']);
  });

  it('orders by tag name when dates are missing', () => {
    const undated = state(
      { 'v1.9': meta(null), 'v1.10': meta(null), 'v1.8': meta(null), 'v1.7': meta(null) },
      ['v1.7', 'v1.8', 'v1.9', 'v1.10'],
    );

    const picked = selectVersions(
      'dll',
      ['v1.7', 'v1.8', 'v1.9', 'v1.10'].map(tag),
      undated,
      LIMITS,
    );

    expect(names(picked)).toEqual(['v1.10', 'v1.9', 'v1.8']);
  });

  it('falls back to the newest tags for a repo without releases', () => {
    const none = state({}, ['v1', 'v2', 'v3', 'v4']);

    const picked = selectVersions('package', ['v1', 'v2', 'v3', 'v4'].map(tag), none, LIMITS);

    expect(names(picked)).toEqual(['v4', 'v3', 'v2']);
    expect(new Set(picked.map((v) => v.kind))).toEqual(new Set(['tag']));
  });

  it('indexes a tag GitHub has not been asked about yet, once, as a plain tag', () => {
    const stale = { ...releases, tags: names0 };
    const fresh = [...tags, tag('v1.7-beta')];

    const picked = selectVersions('dll', fresh, stale, { releases: 1, prereleases: 3 });

    expect(picked.find((v) => v.name === 'v1.7-beta')?.kind).toBe('tag');
  });

  it('ignores tags for the head-only baseline', () => {
    expect(selectVersions('baseline', tags, releases, LIMITS)).toEqual([]);
  });
});
