/**
 * What the indexer asks of upstream: which refs exist, what a commit is, a commit's files on
 * disk, and which tags GitHub calls releases.
 */
import { createGitSources } from './git.ts';
import type { Logger } from './logger.ts';
import { fetchReleases, type ReleaseLookup } from './releases.ts';
import type { TrackedRepo } from './repos.ts';

export interface RemoteTag {
  name: string;
  /** The commit the tag points at (peeled for annotated tags). */
  sha: string;
}

export interface RemoteRefs {
  defaultBranch: string;
  /** The commit the default branch points at. */
  head: string;
  tags: RemoteTag[];
}

export interface CommitInfo {
  /** ISO committer date. */
  date: string;
  subject: string;
}

export interface SourceClient extends ReleaseLookup {
  listRefs(repo: TrackedRepo): Promise<RemoteRefs>;
  /** Reads a commit that `materialize` fetched before. */
  commitInfo(repo: TrackedRepo, sha: string): Promise<CommitInfo>;
  /** Writes the text files of `sha` into `destDir`; returns the bytes written. */
  materialize(repo: TrackedRepo, sha: string, destDir: string): Promise<number>;
  /** Drops the local cache of `repo` when it outgrew `maxBytes`; true if it did. */
  trimCache(repo: TrackedRepo, maxBytes: number): Promise<boolean>;
}

export function createSourceClient(options: {
  dataDir: string;
  token?: string | undefined;
  fetch?: typeof fetch;
  logger?: Logger;
}): SourceClient {
  const git = createGitSources(options.dataDir, 'git', options.logger);
  return {
    ...git,
    releases: (repo) =>
      fetchReleases(repo, {
        token: options.token,
        ...(options.fetch ? { fetch: options.fetch } : {}),
      }),
  };
}
