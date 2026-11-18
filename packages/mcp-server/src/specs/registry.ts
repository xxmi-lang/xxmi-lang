/**
 * The specs the server can answer with: the one bundled with this release (always there) and
 * the ones built per indexed DLL commit, loaded from the volume on first use. `resolve` maps a
 * `dll_ref` to one of them, falling back to the bundled spec with a note while a version's own
 * spec is missing.
 */
import { SpecLookup, type Spec } from '@xxmi-lang/core';
import { CatalogError, type Catalog, type VersionKind } from '../catalog.ts';
import { baselineRepo, dllRepo, fullName, TRACKED_REPOS, type TrackedRepo } from '../repos.ts';
import { isCurrent, type SpecHeader, type SpecStore, type StoredSpec } from './store.ts';

export const DEFAULT_DLL_REF = 'latest-release';
export const BUNDLED_REF = 'bundled';

/** Refs that mean "the newest of a kind"; they fall back to the bundled spec when none exists. */
const SYMBOLIC_REFS: ReadonlySet<string> = new Set(['latest-release', 'latest-prerelease']);

export interface SpecEntry {
  spec: Spec;
  lookup: SpecLookup;
}

export type SpecSlot =
  { state: 'ok'; entry: SpecEntry; header: SpecHeader } | { state: 'failed'; header: SpecHeader };

export interface SpecStatus {
  state: 'ok' | 'pending' | 'failed';
  error?: string;
}

export interface ResolvedSpec extends SpecEntry {
  source: 'extracted' | 'bundled';
  /** What the caller asked for (the default when nothing was passed). */
  dllRef: string;
  /** The DLL commit the spec was extracted from. */
  dllSha: string;
  dllCommitDate: string;
  /** Name of the indexed version (tag or default branch), when the spec is extracted. */
  name?: string;
  tag?: string;
  kind?: VersionKind;
  /** Why the bundled spec answers instead of the requested one, as a clause. */
  fallback?: string;
  note?: string;
}

/** The version a spec is wanted for; `ResolvedRef` of the catalog fits. */
export interface VersionTarget {
  name: string;
  sha: string;
  kind: VersionKind;
}

export interface SpecRegistryOptions {
  store: SpecStore;
  catalog: Catalog;
  /** The spec built into this release. */
  bundled: Spec;
  /** `builderId()`: stored specs of another builder are rebuilt. */
  builder: string;
  repos?: readonly TrackedRepo[];
}

const entryOf = (spec: Spec): SpecEntry => ({ spec, lookup: new SpecLookup(spec) });

const short = (sha: string): string => sha.slice(0, 7);

function slotOf(stored: StoredSpec): SpecSlot | undefined {
  if (stored.status === 'failed') return { state: 'failed', header: stored };
  return stored.spec ? { state: 'ok', entry: entryOf(stored.spec), header: stored } : undefined;
}

export class SpecRegistry {
  readonly bundled: SpecEntry;
  readonly builder: string;
  private readonly slots = new Map<string, SpecSlot>();
  private readonly store: SpecStore;
  private readonly catalog: Catalog;
  private readonly repos: readonly TrackedRepo[];

  constructor(options: SpecRegistryOptions) {
    this.store = options.store;
    this.catalog = options.catalog;
    this.builder = options.builder;
    this.repos = options.repos ?? TRACKED_REPOS;
    this.bundled = entryOf(options.bundled);
  }

  /** The baseline commit stored specs must have been built against; unknown until it is indexed. */
  baselineSha(): string | undefined {
    const repo = baselineRepo(this.repos);
    return repo ? this.catalog.repoState(repo)?.branch.sha : undefined;
  }

  /** The current build result for a DLL commit, or undefined when it has to be (re)built. */
  async slot(dllSha: string): Promise<SpecSlot | undefined> {
    const baselineSha = this.baselineSha();
    if (!baselineSha) return undefined;
    const expected = { dllSha, baselineSha, builder: this.builder };

    const cached = this.slots.get(dllSha);
    if (cached && isCurrent(cached.header, expected)) return cached;
    this.slots.delete(dllSha);

    const stored = await this.store.read(dllSha);
    const slot = stored && isCurrent(stored, expected) ? slotOf(stored) : undefined;
    if (slot) this.slots.set(dllSha, slot);
    return slot;
  }

  async status(dllSha: string): Promise<SpecStatus> {
    const slot = await this.slot(dllSha);
    if (!slot) return { state: 'pending' };
    return slot.state === 'ok'
      ? { state: 'ok' }
      : { state: 'failed', error: slot.header.error ?? 'unknown error' };
  }

  /** Saves a build result and serves it from now on. */
  async record(stored: StoredSpec): Promise<void> {
    await this.store.write(stored);
    const slot = slotOf(stored);
    if (slot) this.slots.set(stored.dllSha, slot);
  }

  /** Drops the specs of DLL commits that are no longer indexed, from memory and from the volume. */
  async retain(keep: ReadonlySet<string>): Promise<string[]> {
    for (const sha of [...this.slots.keys()]) if (!keep.has(sha)) this.slots.delete(sha);
    return this.store.prune(keep);
  }

  /** The spec for a `dll_ref`; unknown refs throw, missing specs fall back to the bundled one. */
  async resolve(ref?: string): Promise<ResolvedSpec> {
    const dllRef = ref?.trim() || DEFAULT_DLL_REF;
    if (dllRef.toLowerCase() === BUNDLED_REF) return this.bundledResult(dllRef);

    const repo = dllRepo(this.repos);
    if (!repo || !this.catalog.isReady(repo)) {
      return this.bundledResult(
        dllRef,
        `requested ${dllRef} but the DLL index is still warming up`,
      );
    }
    let target: VersionTarget;
    try {
      target = this.catalog.resolve(repo, dllRef);
    } catch (error) {
      if (!(error instanceof CatalogError)) throw error;
      if (SYMBOLIC_REFS.has(dllRef.toLowerCase())) {
        return this.bundledResult(dllRef, `requested ${dllRef} but no such version is indexed`);
      }
      throw new CatalogError(
        `${error.message} Also valid: latest-release, latest-prerelease, head, ${BUNDLED_REF}.`,
      );
    }
    return this.forVersion(target, dllRef);
  }

  /** The spec of an already resolved DLL version. */
  async forVersion(target: VersionTarget, dllRef: string): Promise<ResolvedSpec> {
    const slot = await this.slot(target.sha);
    const label = `${target.name} (${short(target.sha)})`;
    if (!slot) return this.bundledResult(dllRef, `requested ${label} is still building`);
    if (slot.state === 'failed') {
      const why = slot.header.error ?? 'unknown error';
      return this.bundledResult(dllRef, `requested ${label} could not be built (${why})`);
    }
    return {
      ...slot.entry,
      source: 'extracted',
      dllRef,
      dllSha: target.sha,
      dllCommitDate: slot.entry.spec.meta.dll.commitDate,
      name: target.name,
      kind: target.kind,
      ...(target.kind === 'head' ? {} : { tag: target.name }),
    };
  }

  private bundledResult(dllRef: string, fallback?: string): ResolvedSpec {
    const { dll } = this.bundled.spec.meta;
    const base = {
      ...this.bundled,
      source: 'bundled' as const,
      dllRef,
      dllSha: dll.commit,
      dllCommitDate: dll.commitDate,
    };
    if (!fallback) return base;
    const repo = dllRepo(this.repos);
    const origin = repo ? fullName(repo) : 'the DLL';
    const sentence = `${fallback[0]?.toUpperCase() ?? ''}${fallback.slice(1)}; answering from the spec bundled with this server (${origin} ${short(dll.commit)}).`;
    return { ...base, fallback, note: sentence };
  }
}
