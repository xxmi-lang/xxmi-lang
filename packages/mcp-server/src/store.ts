/**
 * The data volume (docs/06-mcp-server.md "As built"):
 *
 *   <DATA_DIR>/repos/<owner>/<repo>/<sha>/      checkout of one commit (text files only)
 *   <DATA_DIR>/index/<owner>__<repo>/<sha>.json  library symbols of a package commit
 *   <DATA_DIR>/state.json                        which refs are indexed
 *   <DATA_DIR>/tmp/                              staging area, same file system as the rest
 *
 * Everything becomes visible by a rename, so a crash never leaves a half-written checkout or
 * state file behind.
 */
import { randomBytes } from 'node:crypto';
import { existsSync } from 'node:fs';
import { mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { repoDirName, type TrackedRepo } from './repos.ts';

export class Store {
  readonly dataDir: string;

  constructor(dataDir: string) {
    this.dataDir = dataDir;
  }

  checkoutDir(repo: TrackedRepo, sha: string): string {
    return join(this.dataDir, 'repos', repo.owner, repo.name, sha);
  }

  indexFile(repo: TrackedRepo, sha: string): string {
    return join(this.dataDir, 'index', repoDirName(repo), `${sha}.json`);
  }

  private get statePath(): string {
    return join(this.dataDir, 'state.json');
  }

  hasCheckout(repo: TrackedRepo, sha: string): boolean {
    return existsSync(this.checkoutDir(repo, sha));
  }

  async init(): Promise<void> {
    await mkdir(join(this.dataDir, 'tmp'), { recursive: true });
    await this.clearStaging();
  }

  /** Leftovers of an interrupted run. */
  private async clearStaging(): Promise<void> {
    const tmp = join(this.dataDir, 'tmp');
    for (const name of await readdir(tmp))
      await rm(join(tmp, name), { recursive: true, force: true });
  }

  newStagingDir(repo: TrackedRepo): string {
    return join(this.dataDir, 'tmp', `${repoDirName(repo)}-${randomBytes(6).toString('hex')}`);
  }

  /** Publishes a finished staging directory as the checkout of `sha`. */
  async commitCheckout(repo: TrackedRepo, sha: string, staging: string): Promise<void> {
    const target = this.checkoutDir(repo, sha);
    await mkdir(join(target, '..'), { recursive: true });
    if (existsSync(target)) await rm(staging, { recursive: true, force: true });
    else await rename(staging, target);
  }

  async writeIndex(repo: TrackedRepo, sha: string, value: unknown): Promise<void> {
    await this.writeJsonAtomic(this.indexFile(repo, sha), value);
  }

  async readIndex<T>(repo: TrackedRepo, sha: string): Promise<T | undefined> {
    return this.readJson<T>(this.indexFile(repo, sha));
  }

  async readState<T>(): Promise<T | undefined> {
    return this.readJson<T>(this.statePath);
  }

  async writeState(value: unknown): Promise<void> {
    await this.writeJsonAtomic(this.statePath, value);
  }

  /** Removes checkouts and indexes of `repo` whose sha isn't in `keep`. */
  async prune(repo: TrackedRepo, keep: ReadonlySet<string>): Promise<string[]> {
    const removed: string[] = [];
    const checkouts = join(this.dataDir, 'repos', repo.owner, repo.name);
    for (const sha of await readdir(checkouts).catch(() => [])) {
      if (keep.has(sha)) continue;
      await rm(join(checkouts, sha), { recursive: true, force: true });
      await rm(this.indexFile(repo, sha), { force: true });
      removed.push(sha);
    }
    return removed;
  }

  private async readJson<T>(path: string): Promise<T | undefined> {
    try {
      return JSON.parse(await readFile(path, 'utf8')) as T;
    } catch {
      return undefined;
    }
  }

  private async writeJsonAtomic(path: string, value: unknown): Promise<void> {
    await mkdir(join(path, '..'), { recursive: true });
    const tmp = join(this.dataDir, 'tmp', `${randomBytes(6).toString('hex')}.json`);
    await writeFile(tmp, JSON.stringify(value));
    await rename(tmp, path);
  }
}
