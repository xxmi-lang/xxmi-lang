/**
 * Loads indexed package checkouts through core (`Workspace`, `buildSnapshot`). The same code
 * builds the symbol index at indexing time and answers `library_*` / `lint_ini` later.
 */
import { join } from 'node:path';
import {
  buildSnapshot,
  defaultSpec,
  SpecLookup,
  Workspace,
  type LoadedPackage,
  type SymbolSnapshot,
} from '@xxmi-lang/core';
import type { ResolvedRef } from './catalog.ts';
import { fullName, type TrackedRepo } from './repos.ts';
import type { Store } from './store.ts';

export interface LoadedPackageIndex {
  pkg: LoadedPackage;
  snapshot: SymbolSnapshot;
}

/** Parses the package in `checkout` (the repo root) and builds its symbol snapshot. */
export async function loadPackageCheckout(
  checkout: string,
  repo: TrackedRepo,
  sha: string,
): Promise<LoadedPackageIndex> {
  const root = join(checkout, repo.packageDir ?? '.');
  const workspace = new Workspace({ lookup: new SpecLookup(defaultSpec()) });
  await workspace.loadPackage(root);
  const snapshot = buildSnapshot(workspace, root, {
    name: repo.game ?? repo.name.toLowerCase(),
    repository: `https://github.com/${fullName(repo)}`,
    commit: sha,
  });
  return { pkg: { workspace, root }, snapshot };
}

const CACHE_SIZE = 3;

/** Keeps the few most recently used packages parsed in memory. */
export class PackageCache {
  private readonly entries = new Map<string, Promise<LoadedPackageIndex>>();
  private readonly store: Store;

  constructor(store: Store) {
    this.store = store;
  }

  get(ref: ResolvedRef): Promise<LoadedPackageIndex> {
    const key = `${ref.repo.alias}@${ref.sha}`;
    let entry = this.entries.get(key);
    if (entry) {
      this.entries.delete(key);
    } else {
      entry = loadPackageCheckout(ref.dir, ref.repo, ref.sha);
      entry.catch(() => this.entries.delete(key));
    }
    this.entries.set(key, entry);
    while (this.entries.size > CACHE_SIZE) {
      const oldest = this.entries.keys().next();
      if (oldest.done) break;
      this.entries.delete(oldest.value);
    }
    return entry;
  }

  /** The persisted snapshot, without parsing the package when the index file exists. */
  async snapshot(ref: ResolvedRef): Promise<SymbolSnapshot> {
    const saved = await this.store.readIndex<SymbolSnapshot>(ref.repo, ref.sha);
    return saved ?? (await this.get(ref)).snapshot;
  }
}
