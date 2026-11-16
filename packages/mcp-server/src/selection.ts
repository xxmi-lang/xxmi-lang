/**
 * Which refs of a repo get indexed, per role: the head always, and this picks the tags.
 * See docs/06-mcp-server.md "Versions".
 */
import type { VersionKind, VersionRef } from './catalog.ts';
import { isStale, type ReleasesState } from './releases.ts';
import type { RepoRole } from './repos.ts';
import type { RemoteTag } from './upstream.ts';

export interface SelectionLimits {
  releases: number;
  prereleases: number;
}

const TAGGED_ROLES: ReadonlySet<RepoRole> = new Set(['dll', 'package', 'tools']);

/** Whether tags (and so the GitHub releases lookup) matter for `role`; the baseline is head-only. */
export const tracksTags = (role: RepoRole): boolean => TAGGED_ROLES.has(role);

/** Numeric runs compare as numbers, so `v1.10` is newer than `v1.9`. */
export function compareTagNames(a: string, b: string): number {
  const left = a.match(/\d+|\D+/g) ?? [];
  const right = b.match(/\d+|\D+/g) ?? [];
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    const x = left[i] ?? '';
    const y = right[i] ?? '';
    const order = /^\d/.test(x) && /^\d/.test(y) ? Number(x) - Number(y) : x.localeCompare(y);
    if (order !== 0) return order;
  }
  return left.length - right.length;
}

/** Newest first: by publish date when both have one, else by tag name. */
export function newestFirst(a: VersionRef, b: VersionRef): number {
  if (a.publishedAt && b.publishedAt && a.publishedAt !== b.publishedAt) {
    return a.publishedAt < b.publishedAt ? 1 : -1;
  }
  return compareTagNames(b.name, a.name);
}

function classify(tag: RemoteTag, state: ReleasesState | undefined): VersionRef | undefined {
  const meta = state?.releases[tag.name];
  if (meta?.draft) return undefined;
  const kind: VersionKind = !meta ? 'tag' : meta.prerelease ? 'prerelease' : 'release';
  return {
    name: tag.name,
    sha: tag.sha,
    kind,
    tag: tag.name,
    ...(meta?.publishedAt ? { publishedAt: meta.publishedAt } : {}),
  };
}

const ofKind = (versions: VersionRef[], kind: VersionKind): VersionRef[] =>
  versions.filter((v) => v.kind === kind).sort(newestFirst);

/**
 * The tag versions to index, newest first: the newest releases, then the pre-releases newer
 * than the newest release. Tags GitHub knows nothing about only count when the repo has no
 * releases at all, or while the release map is stale (a new tag may be a pre-release).
 */
export function selectVersions(
  role: RepoRole,
  tags: readonly RemoteTag[],
  state: ReleasesState | undefined,
  limits: SelectionLimits,
): VersionRef[] {
  if (!tracksTags(role)) return [];
  const all = tags.flatMap((t) => classify(t, state) ?? []);
  const releases = ofKind(all, 'release');
  const prereleases = ofKind(all, 'prerelease');
  if (releases.length === 0 && prereleases.length === 0) {
    return ofKind(all, 'tag').slice(0, limits.releases);
  }

  const newest = releases[0];
  const isNewer = (v: VersionRef): boolean => !newest || newestFirst(v, newest) < 0;
  const unclassified = isStale(
    state,
    tags.map((t) => t.name),
  )
    ? ofKind(all, 'tag')
    : [];
  const newer = [...prereleases, ...unclassified].filter(isNewer).sort(newestFirst);
  return [...releases.slice(0, limits.releases), ...newer.slice(0, limits.prereleases)].sort(
    newestFirst,
  );
}
