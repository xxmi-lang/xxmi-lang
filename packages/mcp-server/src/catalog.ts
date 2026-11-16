/**
 * Which refs of which repos are indexed right now, and how tool inputs (`ref`, `repo`) map to
 * a checkout. The indexer is the only writer; tools only read, and see a new commit as soon as
 * the indexer publishes it.
 */
import { findRepo, fullName, TRACKED_REPOS, type TrackedRepo } from './repos.ts';
import type { ReleasesState } from './releases.ts';
import type { Store } from './store.ts';

/** `tag` is a git tag without a GitHub release, or one GitHub hasn't been asked about yet. */
export type VersionKind = 'head' | 'release' | 'prerelease' | 'tag';

export interface VersionRef {
  name: string;
  sha: string;
  kind: VersionKind;
  /** Git tag name; absent for the branch head. */
  tag?: string;
  /** When the GitHub release was published, if there is one. */
  publishedAt?: string;
  committedAt?: string;
  subject?: string;
}

export interface RepoState {
  branch: VersionRef;
  /** Newest first. */
  tags: VersionRef[];
  /** ISO time the default branch was last published. */
  indexedAt: string;
}

export interface StateFile {
  version: 1;
  repos: Record<string, RepoState>;
  /** What GitHub said about each repo's tags, kept until the tag set changes. */
  releases: Record<string, ReleasesState>;
}

export interface ResolvedRef {
  repo: TrackedRepo;
  name: string;
  sha: string;
  dir: string;
  isDefault: boolean;
  kind: VersionKind;
}

export class CatalogError extends Error {}

/** The newest release; repos without GitHub releases fall back to their newest tag. */
export function latestRelease(entry: RepoState): VersionRef | undefined {
  const release = entry.tags.find((t) => t.kind === 'release');
  const hasReleases = entry.tags.some((t) => t.kind === 'release' || t.kind === 'prerelease');
  return release ?? (hasReleases ? undefined : entry.tags.find((t) => t.kind === 'tag'));
}

/** The newest pre-release; the indexed ones are always newer than the newest release. */
export const latestPrerelease = (entry: RepoState): VersionRef | undefined =>
  entry.tags.find((t) => t.kind === 'prerelease');

function findVersion(entry: RepoState, wanted: string): VersionRef | undefined {
  const lower = wanted.toLowerCase();
  if (lower === '' || lower === 'latest' || lower === 'head') return entry.branch;
  if (lower === 'latest-release') return latestRelease(entry);
  if (lower === 'latest-prerelease') return latestPrerelease(entry) ?? latestRelease(entry);

  const all = [entry.branch, ...entry.tags];
  return (
    all.find((r) => r.name.toLowerCase() === lower) ??
    (lower.length >= 7 ? all.find((r) => r.sha.startsWith(lower)) : undefined)
  );
}

export class Catalog {
  private state: StateFile = { version: 1, repos: {}, releases: {} };
  private readonly store: Store;
  private readonly repos: readonly TrackedRepo[];

  constructor(store: Store, repos: readonly TrackedRepo[] = TRACKED_REPOS) {
    this.store = store;
    this.repos = repos;
  }

  /** Loads `state.json`, dropping refs whose checkout vanished from the volume. */
  async load(): Promise<void> {
    // Files from before release tracking have no `releases`, and their tags no `kind`.
    const saved = await this.store.readState<
      Partial<StateFile> & Pick<StateFile, 'version' | 'repos'>
    >();
    if (saved?.version !== 1) return;
    for (const repo of this.repos) {
      const entry = saved.repos[repo.alias];
      if (!entry || !this.store.hasCheckout(repo, entry.branch.sha)) continue;
      this.state.repos[repo.alias] = {
        ...entry,
        branch: { ...entry.branch, kind: 'head' },
        tags: entry.tags
          .filter((t) => this.store.hasCheckout(repo, t.sha))
          .map((t) => ({ ...t, kind: (t as Partial<VersionRef>).kind ?? 'tag' })),
      };
      const releases = saved.releases?.[repo.alias];
      if (releases) this.state.releases[repo.alias] = releases;
    }
  }

  snapshot(): StateFile {
    return this.state;
  }

  /** Publishes a new state (the indexer calls this after the files are in place). */
  async publish(next: StateFile): Promise<void> {
    await this.store.writeState(next);
    this.state = next;
  }

  repoState(repo: TrackedRepo): RepoState | undefined {
    return this.state.repos[repo.alias];
  }

  /**
   * True once every tracked repo but the baseline has its default branch indexed: the baseline
   * only backs DLL comparisons, so a failing one must not keep the server "warming up".
   */
  get ready(): boolean {
    return this.repos.every(
      (r) => r.role === 'baseline' || this.state.repos[r.alias] !== undefined,
    );
  }

  isReady(repo: TrackedRepo): boolean {
    return this.state.repos[repo.alias] !== undefined;
  }

  /** `owner/name` or alias to a tracked repo; throws with the allowed names otherwise. */
  repoByName(query: string): TrackedRepo {
    const found = findRepo(query, this.repos);
    if (found) return found;
    const allowed = this.repos.map((r) => `${r.alias} (${fullName(r)})`).join(', ');
    throw new CatalogError(`Unknown repo "${query}". Tracked repos: ${allowed}.`);
  }

  /**
   * The checkout for `ref`: omitted, `latest` or `head` is the default branch;
   * `latest-release` and `latest-prerelease` follow the releases; otherwise a tag or branch
   * name, or a commit sha prefix (7+ characters) of an indexed ref.
   */
  resolve(repo: TrackedRepo, ref?: string): ResolvedRef {
    const entry = this.repoState(repo);
    if (!entry) {
      throw new CatalogError(
        `${fullName(repo)} is not indexed yet (the server is warming up). Try again in a minute.`,
      );
    }
    const wanted = ref?.trim() ?? '';
    const found = findVersion(entry, wanted);
    if (!found) {
      const names = [entry.branch, ...entry.tags].map((r) => r.name).join(', ');
      throw new CatalogError(
        `ref "${wanted}" is not indexed for ${fullName(repo)}. Indexed: ${names}.`,
      );
    }
    return {
      repo,
      name: found.name,
      sha: found.sha,
      dir: this.store.checkoutDir(repo, found.sha),
      isDefault: found === entry.branch,
      kind: found.kind,
    };
  }

  /** Shas that must stay on disk. */
  keptShas(repo: TrackedRepo): Set<string> {
    const entry = this.repoState(repo);
    return new Set(entry ? [entry.branch.sha, ...entry.tags.map((t) => t.sha)] : []);
  }
}
