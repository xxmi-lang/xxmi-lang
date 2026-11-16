/** A real local git repository standing in for a GitHub repo; tests fetch from it by path. */
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import type { TrackedRepo } from '../src/repos.ts';
import { tempDir } from './helpers.ts';

const IDENTITY = {
  GIT_AUTHOR_NAME: 'Test',
  GIT_AUTHOR_EMAIL: 'test@example.com',
  GIT_COMMITTER_NAME: 'Test',
  GIT_COMMITTER_EMAIL: 'test@example.com',
};

export class LocalUpstream {
  /** The bare repository that the server fetches from. */
  readonly bare: string;
  private readonly work: string;

  constructor() {
    const root = tempDir();
    this.bare = join(root, 'upstream.git');
    this.work = join(root, 'work');
    this.git(root, 'init', '--bare', '--quiet', '--initial-branch=main', this.bare);
    this.git(root, 'init', '--quiet', '--initial-branch=main', this.work);
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
    return this.git(this.work, 'rev-parse', 'HEAD');
  }

  /** Tags `rev` (default: the last commit); moves the tag if it exists. */
  tag(name: string, options: { annotated?: boolean; rev?: string } = {}): void {
    const args = options.annotated ? ['tag', '-f', '-a', '-m', name] : ['tag', '-f'];
    this.git(this.work, ...args, name, options.rev ?? 'HEAD');
    this.git(this.work, 'push', '--quiet', '--force', 'origin', `refs/tags/${name}`);
  }

  /** Creates a tag `git tag` would refuse (such as `-evil`) by writing the ref directly. */
  forceTag(name: string, rev = 'HEAD'): void {
    this.git(this.work, 'update-ref', `refs/tags/${name}`, rev);
    this.git(this.work, 'push', '--quiet', '--force', 'origin', `refs/tags/${name}`);
  }

  deleteTag(name: string): void {
    this.git(this.work, 'tag', '--delete', name);
    this.git(this.work, 'push', '--quiet', 'origin', `:refs/tags/${name}`);
  }

  /** The object a ref points at, without peeling annotated tags. */
  revParse(rev: string): string {
    return this.git(this.work, 'rev-parse', rev);
  }

  private git(cwd: string, ...args: string[]): string {
    const out = execFileSync('git', args, {
      cwd,
      encoding: 'utf8',
      env: { ...process.env, ...IDENTITY },
    });
    return out.trim();
  }
}
