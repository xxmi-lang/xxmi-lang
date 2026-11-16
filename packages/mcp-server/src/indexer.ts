/**
 * Polls the tracked repos and keeps the volume current (docs/06-mcp-server.md "Architecture").
 * Per pass: `git ls-remote` each repo, work out the wanted versions, fetch and extract each
 * missing commit, build the symbol index for packages, and only then publish the new state.
 * Readers keep the old checkout until the swap, and a failed fetch leaves everything as it was.
 */
import { rm } from 'node:fs/promises';
import type { SymbolSnapshot } from '@xxmi-lang/core';
import { Catalog, type RepoState, type StateFile, type VersionRef } from './catalog.ts';
import { errorMessage, type Logger } from './logger.ts';
import { loadPackageCheckout } from './packages.ts';
import { refreshReleases, type ReleasesState } from './releases.ts';
import { fullName, type TrackedRepo } from './repos.ts';
import { selectVersions, tracksTags, type SelectionLimits } from './selection.ts';
import type { Store } from './store.ts';
import type { RemoteRefs, SourceClient } from './upstream.ts';

export interface IndexerOptions {
  store: Store;
  catalog: Catalog;
  sources: SourceClient;
  logger: Logger;
  repos: readonly TrackedRepo[];
  limits: SelectionLimits;
  gitCacheMaxMb: number;
  pollIntervalMin: number;
  /** Builds the symbol snapshot of a package checkout; replaceable in tests. */
  buildIndex?: (checkout: string, repo: TrackedRepo, sha: string) => Promise<SymbolSnapshot>;
  now?: () => number;
}

export interface PassResult {
  /** `owner/name@ref` for everything newly published. */
  published: string[];
  errors: string[];
}

interface Head {
  refs: RemoteRefs;
  version: VersionRef;
}

/** What to write into the next `state.json` for one repo; unset parts stay as they are. */
interface StatePatch {
  branch: VersionRef;
  tags?: VersionRef[];
  releases?: ReleasesState;
}

const RETRY_WHILE_NOT_READY_MS = 60_000;

/** Poll interval, or a quick retry while the first index is still missing (GitHub hiccup). */
export function nextDelayMs(result: PassResult, ready: boolean, pollIntervalMin: number): number {
  const interval = pollIntervalMin * 60_000;
  return result.errors.length > 0 && !ready
    ? Math.min(RETRY_WHILE_NOT_READY_MS, interval)
    : interval;
}

export class Indexer {
  private readonly o: IndexerOptions;
  private timer: NodeJS.Timeout | undefined;
  private running: Promise<PassResult> | undefined;
  private started = false;
  private forceReleases = false;

  constructor(options: IndexerOptions) {
    this.o = options;
  }

  start(): void {
    if (this.o.pollIntervalMin <= 0 || this.started) return;
    this.started = true;
    const tick = (): void => {
      void this.runOnce().then((result) => {
        if (!this.started) return;
        const delay = nextDelayMs(result, this.o.catalog.ready, this.o.pollIntervalMin);
        this.timer = setTimeout(tick, delay);
        this.timer.unref();
      });
    };
    tick();
  }

  stop(): void {
    this.started = false;
    if (this.timer) clearTimeout(this.timer);
    this.timer = undefined;
  }

  /** One polling pass. Calls while a pass runs share its result. */
  runOnce(): Promise<PassResult> {
    this.running ??= this.pass().finally(() => {
      this.running = undefined;
    });
    return this.running;
  }

  /** A pass that also asks GitHub about releases again, even if the tags didn't change. */
  refresh(): Promise<PassResult> {
    this.forceReleases = true;
    return this.running ? this.running.then(() => this.runOnce()) : this.runOnce();
  }

  private async pass(): Promise<PassResult> {
    const result: PassResult = { published: [], errors: [] };
    const started = Date.now();
    const force = this.forceReleases;
    this.forceReleases = false;
    const heads = new Map<string, Head>();
    // The commits readers may still be using: they survive one more pass.
    const grace = this.keptNow();

    for (const repo of this.o.repos) {
      await this.guard(repo, result, () => this.indexHead(repo, heads, result));
    }

    // Tags come second so the server is ready as soon as every default branch is in.
    for (const repo of this.o.repos) {
      const head = heads.get(repo.alias);
      if (!head || !tracksTags(repo.role)) continue;
      await this.guard(repo, result, () => this.indexTags(repo, head, result, force));
    }

    await this.prune(grace);
    this.o.logger.info('index pass finished', {
      ms: Date.now() - started,
      published: result.published,
      errors: result.errors.length,
    });
    return result;
  }

  private async guard(
    repo: TrackedRepo,
    result: PassResult,
    step: () => Promise<void>,
  ): Promise<void> {
    try {
      await step();
    } catch (error) {
      const message = `${fullName(repo)}: ${errorMessage(error)}`;
      result.errors.push(message);
      this.o.logger.warn('index step failed', { repo: fullName(repo), error: errorMessage(error) });
    }
  }

  private async indexHead(
    repo: TrackedRepo,
    heads: Map<string, Head>,
    result: PassResult,
  ): Promise<void> {
    const refs = await this.o.sources.listRefs(repo);
    const wanted: VersionRef = { name: refs.defaultBranch, sha: refs.head, kind: 'head' };

    // Nothing is published for a head that couldn't be fetched: readers keep the old one.
    const version = await this.ensure(repo, wanted, result);
    if (!version) return;
    heads.set(repo.alias, { refs, version });
    await this.publishState(repo, { branch: version });
  }

  private async indexTags(
    repo: TrackedRepo,
    head: Head,
    result: PassResult,
    force: boolean,
  ): Promise<void> {
    const releases = await refreshReleases(
      this.o.sources,
      repo,
      head.refs.tags.map((t) => t.name),
      this.o.catalog.snapshot().releases[repo.alias],
      { logger: this.o.logger, now: (this.o.now ?? Date.now)(), force },
    );

    const wanted = selectVersions(repo.role, head.refs.tags, releases, this.o.limits);
    const present: VersionRef[] = [];
    for (const version of wanted) {
      const ensured = await this.ensure(repo, version, result);
      if (ensured) present.push(ensured);
    }
    await this.publishState(repo, { branch: head.version, tags: present, releases });
  }

  /** Makes sure `version` is on disk; returns it with its commit info, or undefined on failure. */
  private async ensure(
    repo: TrackedRepo,
    version: VersionRef,
    result: PassResult,
  ): Promise<VersionRef | undefined> {
    if (this.o.store.hasCheckout(repo, version.sha)) {
      return { ...version, ...this.knownCommitInfo(repo, version.sha) };
    }
    try {
      const info = await this.fetchAndIndex(repo, version);
      result.published.push(`${fullName(repo)}@${version.name}`);
      return { ...version, ...info };
    } catch (error) {
      result.errors.push(`${fullName(repo)}@${version.name}: ${errorMessage(error)}`);
      this.o.logger.warn('fetch failed', {
        repo: fullName(repo),
        ref: version.name,
        error: errorMessage(error),
      });
      return undefined;
    }
  }

  private async fetchAndIndex(
    repo: TrackedRepo,
    version: VersionRef,
  ): Promise<Pick<VersionRef, 'committedAt' | 'subject'>> {
    const started = Date.now();
    const staging = this.o.store.newStagingDir(repo);
    try {
      const bytes = await this.o.sources.materialize(repo, version.sha, staging);
      const commit = await this.o.sources.commitInfo(repo, version.sha);
      if (repo.role === 'package') await this.writePackageIndex(repo, version.sha, staging);
      await this.o.store.commitCheckout(repo, version.sha, staging);
      this.o.logger.info('indexed', {
        repo: fullName(repo),
        ref: version.name,
        sha: version.sha,
        bytes,
        ms: Date.now() - started,
      });
      return { committedAt: commit.date, subject: commit.subject };
    } finally {
      await rm(staging, { recursive: true, force: true });
    }
  }

  private async writePackageIndex(repo: TrackedRepo, sha: string, checkout: string): Promise<void> {
    const build =
      this.o.buildIndex ?? (async (dir, r, s) => (await loadPackageCheckout(dir, r, s)).snapshot);
    await this.o.store.writeIndex(repo, sha, await build(checkout, repo, sha));
  }

  /** Commit details of a version already published, so unchanged commits aren't re-read. */
  private knownCommitInfo(
    repo: TrackedRepo,
    sha: string,
  ): Pick<VersionRef, 'committedAt' | 'subject'> {
    const entry = this.o.catalog.repoState(repo);
    const known = [entry?.branch, ...(entry?.tags ?? [])].find((v) => v?.sha === sha);
    return {
      ...(known?.committedAt ? { committedAt: known.committedAt } : {}),
      ...(known?.subject ? { subject: known.subject } : {}),
    };
  }

  /** Swaps the catalog to the new refs, keeping what the patch leaves out. */
  private async publishState(repo: TrackedRepo, patch: StatePatch): Promise<void> {
    const current = this.o.catalog.snapshot();
    const previous = current.repos[repo.alias];
    const entry: RepoState = {
      branch: patch.branch,
      tags: patch.tags ?? previous?.tags ?? [],
      indexedAt:
        previous?.branch.sha === patch.branch.sha ? previous.indexedAt : new Date().toISOString(),
    };
    const next: StateFile = {
      version: 1,
      repos: { ...current.repos, [repo.alias]: entry },
      releases: patch.releases
        ? { ...current.releases, [repo.alias]: patch.releases }
        : current.releases,
    };
    await this.o.catalog.publish(next);
  }

  private keptNow(): Map<string, Set<string>> {
    return new Map(this.o.repos.map((r) => [r.alias, this.o.catalog.keptShas(r)]));
  }

  private async prune(grace: ReadonlyMap<string, ReadonlySet<string>>): Promise<void> {
    for (const repo of this.o.repos) {
      try {
        const keep = new Set([...this.o.catalog.keptShas(repo), ...(grace.get(repo.alias) ?? [])]);
        const removed = await this.o.store.prune(repo, keep);
        if (removed.length > 0) this.o.logger.info('pruned', { repo: fullName(repo), removed });
        await this.trimGitCache(repo);
      } catch (error) {
        this.o.logger.warn('prune failed', { repo: fullName(repo), error: errorMessage(error) });
      }
    }
  }

  /** The bare clone only grows; past the limit it is dropped and the next fetch rebuilds it. */
  private async trimGitCache(repo: TrackedRepo): Promise<void> {
    const maxBytes = this.o.gitCacheMaxMb * 1024 * 1024;
    if (await this.o.sources.trimCache(repo, maxBytes)) {
      this.o.logger.info('git cache dropped', { repo: fullName(repo) });
    }
  }
}
