/**
 * Which refs of which repos are indexed right now, and how tool inputs (`ref`, `repo`) map to
 * a checkout. The indexer is the only writer; tools only read, and see a new commit as soon as
 * the indexer publishes it.
 */
import { findRepo, fullName, TRACKED_REPOS, type TrackedRepo } from './repos.ts';
import type { RemoteRef } from './github.ts';
import type { Store } from './store.ts';

export interface RepoState {
  branch: RemoteRef;
  tags: RemoteRef[];
  /** ISO time the default branch was last published. */
  indexedAt: string;
}

export interface StateFile {
  version: 1;
  repos: Record<string, RepoState>;
}

export interface ResolvedRef {
  repo: TrackedRepo;
  name: string;
  sha: string;
  dir: string;
  isDefault: boolean;
}

export class CatalogError extends Error {}

export class Catalog {
  private state: StateFile = { version: 1, repos: {} };
  private readonly store: Store;
  private readonly repos: readonly TrackedRepo[];

  constructor(store: Store, repos: readonly TrackedRepo[] = TRACKED_REPOS) {
    this.store = store;
    this.repos = repos;
  }

  /** Loads `state.json`, dropping refs whose checkout vanished from the volume. */
  async load(): Promise<void> {
    const saved = await this.store.readState<StateFile>();
    if (saved?.version !== 1) return;
    for (const repo of this.repos) {
      const entry = saved.repos[repo.alias];
      if (!entry || !this.store.hasCheckout(repo, entry.branch.sha)) continue;
      this.state.repos[repo.alias] = {
        ...entry,
        tags: entry.tags.filter((t) => this.store.hasCheckout(repo, t.sha)),
      };
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

  /** True once every tracked repo has its default branch indexed. */
  get ready(): boolean {
    return this.repos.every((r) => this.state.repos[r.alias] !== undefined);
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
   * The checkout for `ref`: omitted or `latest` is the default branch; otherwise a tag or branch
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
    const all = [
      { ...entry.branch, isDefault: true },
      ...entry.tags.map((t) => ({ ...t, isDefault: false })),
    ];
    const lower = wanted.toLowerCase();
    const found =
      wanted === '' || lower === 'latest'
        ? all[0]
        : (all.find((r) => r.name.toLowerCase() === lower) ??
          (lower.length >= 7 ? all.find((r) => r.sha.startsWith(lower)) : undefined));
    if (!found) {
      throw new CatalogError(
        `ref "${wanted}" is not indexed for ${fullName(repo)}. Indexed: ${all.map((r) => r.name).join(', ')}.`,
      );
    }
    return {
      repo,
      name: found.name,
      sha: found.sha,
      dir: this.store.checkoutDir(repo, found.sha),
      isDefault: found.isDefault,
    };
  }

  /** Shas that must stay on disk. */
  keptShas(repo: TrackedRepo): Set<string> {
    const entry = this.repoState(repo);
    return new Set(entry ? [entry.branch.sha, ...entry.tags.map((t) => t.sha)] : []);
  }
}
