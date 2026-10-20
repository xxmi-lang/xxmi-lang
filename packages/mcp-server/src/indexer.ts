/**
 * Polls the tracked repos and keeps the volume current (docs/06-mcp-server.md "Architecture").
 * Per pass: ask GitHub for the default branch head and the newest tags, download a tarball for
 * each commit that isn't on disk, build the symbol index for packages, and only then publish
 * the new state. Readers keep using the old checkout until the rename/state swap, and a failed
 * download leaves everything as it was.
 */
import { rm } from 'node:fs/promises';
import type { SymbolSnapshot } from '@xxmi-lang/core';
import { Catalog, type RepoState, type StateFile } from './catalog.ts';
import { extractTarball } from './extract.ts';
import type { GitHubClient, RemoteRef } from './github.ts';
import { errorMessage, type Logger } from './logger.ts';
import { loadPackageCheckout } from './packages.ts';
import { fullName, type TrackedRepo } from './repos.ts';
import type { Store } from './store.ts';

export interface IndexerOptions {
  store: Store;
  catalog: Catalog;
  github: GitHubClient;
  logger: Logger;
  repos: readonly TrackedRepo[];
  indexedTags: number;
  pollIntervalMin: number;
  /** Builds the symbol snapshot of a package checkout; replaceable in tests. */
  buildIndex?: (checkout: string, repo: TrackedRepo, sha: string) => Promise<SymbolSnapshot>;
}

export interface PassResult {
  /** `owner/name@ref` for everything newly published. */
  published: string[];
  errors: string[];
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

  private async pass(): Promise<PassResult> {
    const result: PassResult = { published: [], errors: [] };
    const started = Date.now();
    const heads = new Map<string, RemoteRef>();
    // The commits readers may still be using: they survive one more pass.
    const grace = this.keptNow();

    for (const repo of this.o.repos) {
      await this.guard(repo, result, async () => {
        const head = await this.o.github.head(repo);
        // Nothing is published for a head that couldn't be fetched: readers keep the old one.
        if (!(await this.ensure(repo, head, result))) return;
        heads.set(repo.alias, head);
        await this.publishState(repo, head, undefined);
      });
    }

    // Tags come second so the server is ready as soon as every default branch is in.
    for (const repo of this.o.repos) {
      const head = heads.get(repo.alias);
      if (!head) continue;
      await this.guard(repo, result, async () => {
        const tags = await this.o.github.tags(repo, this.o.indexedTags);
        const present: RemoteRef[] = [];
        for (const tag of tags) {
          if (tag.sha === head.sha) {
            present.push(tag);
            continue;
          }
          if (await this.ensure(repo, tag, result)) present.push(tag);
        }
        await this.publishState(repo, head, present);
      });
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

  /** Makes sure `ref` is on disk; returns false when it couldn't be fetched. */
  private async ensure(repo: TrackedRepo, ref: RemoteRef, result: PassResult): Promise<boolean> {
    if (this.o.store.hasCheckout(repo, ref.sha)) return true;
    try {
      await this.fetchAndIndex(repo, ref);
      result.published.push(`${fullName(repo)}@${ref.name}`);
      return true;
    } catch (error) {
      result.errors.push(`${fullName(repo)}@${ref.name}: ${errorMessage(error)}`);
      this.o.logger.warn('fetch failed', {
        repo: fullName(repo),
        ref: ref.name,
        error: errorMessage(error),
      });
      return false;
    }
  }

  private async fetchAndIndex(repo: TrackedRepo, ref: RemoteRef): Promise<void> {
    const started = Date.now();
    const staging = this.o.store.newStagingDir(repo);
    try {
      const bytes = await extractTarball(
        await this.o.github.tarball(repo, ref.sha),
        staging,
        repo.role,
      );
      if (repo.role === 'package') {
        const build =
          this.o.buildIndex ??
          (async (dir, r, sha) => (await loadPackageCheckout(dir, r, sha)).snapshot);
        await this.o.store.writeIndex(repo, ref.sha, await build(staging, repo, ref.sha));
      }
      await this.o.store.commitCheckout(repo, ref.sha, staging);
      this.o.logger.info('indexed', {
        repo: fullName(repo),
        ref: ref.name,
        sha: ref.sha,
        bytes,
        ms: Date.now() - started,
      });
    } finally {
      await rm(staging, { recursive: true, force: true });
    }
  }

  /** Swaps the catalog to the new refs, keeping the previous tags if tags weren't fetched. */
  private async publishState(
    repo: TrackedRepo,
    head: RemoteRef,
    tags: RemoteRef[] | undefined,
  ): Promise<void> {
    const current = this.o.catalog.snapshot();
    const previous = current.repos[repo.alias];
    const entry: RepoState = {
      branch: head,
      tags: tags ?? previous?.tags ?? [],
      indexedAt: previous?.branch.sha === head.sha ? previous.indexedAt : new Date().toISOString(),
    };
    const next: StateFile = { version: 1, repos: { ...current.repos, [repo.alias]: entry } };
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
      } catch (error) {
        this.o.logger.warn('prune failed', { repo: fullName(repo), error: errorMessage(error) });
      }
    }
  }
}
