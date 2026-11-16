/** A real local git repository standing in for a GitHub repo; tests fetch from it by path. */
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import type { TrackedRepo } from '../src/repos.ts';
import { tempDir } from './helpers.ts';

// The machine's git config (autocrlf, signing, hooks of a global template) must not leak in.
const ISOLATION = {
  GIT_CONFIG_NOSYSTEM: '1',
  GIT_AUTHOR_NAME: 'Test',
  GIT_AUTHOR_EMAIL: 'test@example.com',
  GIT_COMMITTER_NAME: 'Test',
  GIT_COMMITTER_EMAIL: 'test@example.com',
};

const REPO_CONFIG: [string, string][] = [
  ['user.name', 'Test'],
  ['user.email', 'test@example.com'],
  ['core.autocrlf', 'false'],
  ['core.safecrlf', 'false'],
  ['commit.gpgsign', 'false'],
  ['tag.gpgsign', 'false'],
];

const FIRST_COMMIT_MS = Date.UTC(2026, 0, 1);

let emptyConfig: string | undefined;

/** Git cannot open the null device as a config file on Windows, so an empty real file stands in. */
function emptyGitConfig(): string {
  if (!emptyConfig) {
    emptyConfig = join(tempDir(), 'gitconfig-empty');
    writeFileSync(emptyConfig, '');
  }
  return emptyConfig;
}

export class LocalUpstream {
  /** The bare repository that the server fetches from. */
  readonly bare: string;
  private readonly work: string;
  /** Commits and tags get strictly increasing dates, so the history never depends on the clock. */
  private clock = FIRST_COMMIT_MS;

  constructor() {
    const root = tempDir();
    this.bare = join(root, 'upstream.git');
    this.work = join(root, 'work');
    this.git(root, 'init', '--bare', '--quiet', '--initial-branch=main', this.bare);
    this.git(root, 'init', '--quiet', '--initial-branch=main', this.work);
    for (const [key, value] of REPO_CONFIG) this.git(this.work, 'config', key, value);
    this.git(this.work, 'remote', 'add', 'origin', this.bare);
  }

  /** Tracked-repo config that points at this repository. */
  repo(overrides: Partial<TrackedRepo> = {}): TrackedRepo {
    return {
      alias: 'up',
      owner: 'test',
      name: 'upstream',
      role: 'tools',
      url: this.bare,
      ...overrides,
    };
  }

  /** Commits `files` (path to content) on main and pushes; returns the commit sha. */
  commit(files: Record<string, string>, message = 'change'): string {
    for (const [path, content] of Object.entries(files)) {
      const target = join(this.work, path);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, content);
    }
    this.git(this.work, 'add', '--all');
    this.git(this.work, 'commit', '--quiet', '-m', message);
    this.git(this.work, 'push', '--quiet', 'origin', 'main');
    this.expectRemote('refs/heads/main', 'HEAD');
    return this.git(this.work, 'rev-parse', 'HEAD');
  }

  /** Tags `rev` (default: the last commit); moves the tag if it exists. */
  tag(name: string, options: { annotated?: boolean; rev?: string } = {}): void {
    const args = options.annotated ? ['tag', '-f', '-a', '-m', name] : ['tag', '-f'];
    this.git(this.work, ...args, name, options.rev ?? 'HEAD');
    this.pushTag(name);
  }

  /** Creates a tag `git tag` would refuse (such as `-evil`) by writing the ref directly. */
  forceTag(name: string, rev = 'HEAD'): void {
    this.git(this.work, 'update-ref', `refs/tags/${name}`, rev);
    this.pushTag(name);
  }

  deleteTag(name: string): void {
    this.git(this.work, 'tag', '--delete', name);
    this.git(this.work, 'push', '--quiet', 'origin', `:refs/tags/${name}`);
  }

  /** The object a ref points at, without peeling annotated tags. */
  revParse(rev: string): string {
    return this.git(this.work, 'rev-parse', rev);
  }

  private pushTag(name: string): void {
    const ref = `refs/tags/${name}`;
    this.git(this.work, 'push', '--quiet', '--force', 'origin', `+${ref}:${ref}`);
    this.expectRemote(ref, ref);
  }

  /** Fails the test setup, not a later assertion, when a push did not land. */
  private expectRemote(ref: string, local: string): void {
    const remote = this.git(this.bare, 'rev-parse', ref);
    const wanted = this.git(this.work, 'rev-parse', local);
    if (remote !== wanted) throw new Error(`push of ${ref} did not land: ${remote} != ${wanted}`);
  }

  private git(cwd: string, ...args: string[]): string {
    this.clock += 60_000;
    const date = new Date(this.clock).toISOString();
    const out = execFileSync('git', args, {
      cwd,
      encoding: 'utf8',
      env: {
        ...process.env,
        ...ISOLATION,
        GIT_CONFIG_GLOBAL: emptyGitConfig(),
        GIT_AUTHOR_DATE: date,
        GIT_COMMITTER_DATE: date,
      },
    });
    return out.trim();
  }
}
