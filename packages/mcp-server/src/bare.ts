/**
 * The bare repositories the server fetches into, one per tracked repo
 * (`<DATA_DIR>/git/<owner>/<repo>.git`). They only ever hold shallow fetches of the commits we
 * index, and are deleted and rebuilt when damaged or too big.
 */
import { lstat, mkdir, readdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { ExecError, run, stream, type ExecOptions, type StreamedCommand } from './exec.ts';
import { errorMessage, type Logger } from './logger.ts';
import { fullName, repoUrl, type TrackedRepo } from './repos.ts';

/** The git executable, optionally followed by arguments that go before every call's own. */
export type GitCommand = string | readonly [string, ...string[]];

export const LIST_TIMEOUT_MS = 30_000;
export const TRANSFER_TIMEOUT_MS = 180_000;

async function treeBytes(dir: string): Promise<number> {
  let total = 0;
  const entries = await readdir(dir, { recursive: true, withFileTypes: true }).catch(() => []);
  for (const entry of entries) {
    if (entry.isFile()) total += (await lstat(join(entry.parentPath, entry.name))).size;
  }
  return total;
}

export class BareCaches {
  private readonly dataDir: string;
  private readonly command: string;
  private readonly prefix: readonly string[];
  private readonly logger: Logger;
  /** Repos whose cache was already rebuilt in the current pass: a second rebuild can't help. */
  private readonly recreated = new Set<string>();

  constructor(dataDir: string, git: GitCommand, logger: Logger) {
    this.dataDir = dataDir;
    [this.command = 'git', ...this.prefix] = typeof git === 'string' ? [git] : git;
    this.logger = logger;
  }

  runGit(args: readonly string[], o: ExecOptions): Promise<string> {
    return run(this.command, [...this.prefix, ...args], o);
  }

  streamGit(args: readonly string[], o: ExecOptions): StreamedCommand {
    return stream(this.command, [...this.prefix, ...args], o);
  }

  dir(repo: TrackedRepo): string {
    return join(this.dataDir, 'git', repo.owner, `${repo.name}.git`);
  }

  inRepo(repo: TrackedRepo, args: readonly string[], label: string, timeoutMs: number) {
    return this.runGit(args, { cwd: this.dir(repo), timeoutMs, label });
  }

  /** Gives `repo` a fresh rebuild allowance; called when a poll pass starts on it. */
  newPass(repo: TrackedRepo): void {
    this.recreated.delete(fullName(repo));
  }

  /**
   * Runs `op` against a healthy cache; after a git failure that is not a timeout, rebuilds the
   * cache and runs it once more. Network outages fail the same way twice and then propagate.
   */
  async withRecovery<T>(repo: TrackedRepo, op: () => Promise<T>): Promise<T> {
    await this.ensure(repo);
    try {
      return await op();
    } catch (error) {
      if (!(error instanceof ExecError) || error.timedOut || this.recreated.has(fullName(repo))) {
        throw error;
      }
      await this.recreate(repo, errorMessage(error));
      return await op();
    }
  }

  /** Makes sure `sha` is in the cache, fetching it (or the ref it was listed under) if not. */
  async ensureCommit(repo: TrackedRepo, sha: string, refHint?: string): Promise<void> {
    if (await this.hasCommit(repo, sha)) return;

    try {
      await this.shallowFetch(repo, sha);
    } catch (error) {
      // Servers without protocol v2 may refuse a fetch by sha; the ref it was listed under works.
      if (!refHint) throw error;
      await this.shallowFetch(repo, refHint);
    }
    if (!(await this.hasCommit(repo, sha))) throw new Error(`${sha} is not on the remote any more`);
  }

  /** Drops the cache when it outgrew `maxBytes`; true if it did. */
  async trim(repo: TrackedRepo, maxBytes: number): Promise<boolean> {
    if ((await treeBytes(this.dir(repo))) <= maxBytes) return false;
    await rm(this.dir(repo), { recursive: true, force: true });
    return true;
  }

  private async ensure(repo: TrackedRepo): Promise<void> {
    const empty = (await readdir(this.dir(repo)).catch(() => [])).length === 0;
    if (empty) await this.create(repo);
    else if (!(await this.healthy(repo))) await this.recreate(repo, 'failed the health check');
  }

  private async create(repo: TrackedRepo): Promise<void> {
    const dir = this.dir(repo);
    await mkdir(dir, { recursive: true });
    await this.runGit(['init', '--bare', '--quiet', dir], {
      timeoutMs: LIST_TIMEOUT_MS,
      label: 'git init',
    });
    await this.inRepo(
      repo,
      ['remote', 'add', 'origin', repoUrl(repo)],
      'git remote',
      LIST_TIMEOUT_MS,
    );
  }

  private async recreate(repo: TrackedRepo, reason: string): Promise<void> {
    this.recreated.add(fullName(repo));
    this.logger.warn('git cache re-created', { repo: fullName(repo), reason });
    await rm(this.dir(repo), { recursive: true, force: true });
    await this.create(repo);
  }

  /** A bare repository whose `origin` is the expected URL; anything else is a half-made cache. */
  private async healthy(repo: TrackedRepo): Promise<boolean> {
    try {
      const bare = await this.inRepo(
        repo,
        ['rev-parse', '--is-bare-repository'],
        'git rev-parse',
        LIST_TIMEOUT_MS,
      );
      const url = await this.inRepo(
        repo,
        ['remote', 'get-url', 'origin'],
        'git remote',
        LIST_TIMEOUT_MS,
      );
      return bare.trim() === 'true' && url.trim() === repoUrl(repo);
    } catch (error) {
      if (error instanceof ExecError && error.timedOut) throw error;
      return false;
    }
  }

  private hasCommit(repo: TrackedRepo, sha: string): Promise<boolean> {
    return this.inRepo(
      repo,
      ['cat-file', '-e', '--end-of-options', `${sha}^{commit}`],
      'git cat-file',
      LIST_TIMEOUT_MS,
    ).then(
      () => true,
      () => false,
    );
  }

  private shallowFetch(repo: TrackedRepo, what: string): Promise<string> {
    const args = ['fetch', '--depth', '1', '--no-tags', '--end-of-options', 'origin', what];
    return this.inRepo(
      repo,
      ['-c', 'protocol.version=2', ...args],
      'git fetch',
      TRANSFER_TIMEOUT_MS,
    );
  }
}
