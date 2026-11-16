/**
 * Git-backed upstream access: `ls-remote` for refs and `git archive` for files, never a checkout.
 * The bare caches it fetches into are in bare.ts.
 */
import { setTimeout as sleep } from 'node:timers/promises';
import { BareCaches, LIST_TIMEOUT_MS, TRANSFER_TIMEOUT_MS, type GitCommand } from './bare.ts';
import { extractTar } from './extract.ts';
import { silentLogger, type Logger } from './logger.ts';
import { fullName, repoUrl, type TrackedRepo } from './repos.ts';
import type { CommitInfo, RemoteRefs, SourceClient } from './upstream.ts';

export type GitSources = Omit<SourceClient, 'releases'>;

const SHA = /^[0-9a-f]{40}$/;
const GIT_EXIT_GRACE_MS = 2_000;

/** Parses `git ls-remote --symref <url> HEAD 'refs/heads/*' 'refs/tags/*'`. */
export function parseLsRemote(output: string): RemoteRefs {
  let symref: string | undefined;
  let headSha: string | undefined;
  const heads = new Map<string, string>();
  const tags = new Map<string, string>();
  const peeled = new Map<string, string>();

  for (const line of output.split('\n')) {
    const sym = /^ref: (\S+)\tHEAD$/.exec(line);
    if (sym) {
      symref = sym[1];
      continue;
    }
    const [sha = '', ref = ''] = line.split('\t');
    if (!SHA.test(sha)) continue;
    if (ref === 'HEAD') headSha = sha;
    else if (ref.startsWith('refs/heads/')) heads.set(ref.slice('refs/heads/'.length), sha);
    else if (ref.endsWith('^{}')) peeled.set(ref.slice('refs/tags/'.length, -3), sha);
    else if (ref.startsWith('refs/tags/')) tags.set(ref.slice('refs/tags/'.length), sha);
  }

  const defaultBranch = symref?.replace(/^refs\/heads\//, '');
  const head = defaultBranch ? (heads.get(defaultBranch) ?? headSha) : undefined;
  if (!defaultBranch || !head) throw new Error('remote has no default branch (empty repository?)');
  return {
    defaultBranch,
    head,
    tags: [...tags].map(([name, sha]) => ({ name, sha: peeled.get(name) ?? sha })),
  };
}

/** Streams `git archive` of `sha` into the extractor; a git failure explains a cut tar best. */
async function extractArchive(
  bare: BareCaches,
  repo: TrackedRepo,
  sha: string,
  destDir: string,
): Promise<number> {
  const archive = bare.streamGit(['archive', '--format=tar', '--end-of-options', sha], {
    cwd: bare.dir(repo),
    timeoutMs: TRANSFER_TIMEOUT_MS,
    label: 'git archive',
  });
  const extraction = extractTar(archive.stdout, destDir, repo.role).catch(
    async (error: unknown) => {
      // The stream often ends because git failed; let it report why before it is killed.
      await Promise.race([archive.exit.catch(() => undefined), sleep(GIT_EXIT_GRACE_MS)]);
      archive.kill();
      throw error;
    },
  );

  const [extracted, exited] = await Promise.allSettled([extraction, archive.exit]);
  if (exited.status === 'rejected' && !archive.killed) throw exited.reason;
  if (extracted.status === 'rejected') throw extracted.reason;
  return extracted.value;
}

export function createGitSources(
  dataDir: string,
  git: GitCommand = 'git',
  logger: Logger = silentLogger,
): GitSources {
  const bare = new BareCaches(dataDir, git, logger);
  /** Which ref pointed at a commit at the last `listRefs`: the fallback when a sha can't be fetched. */
  const refHints = new Map<string, Map<string, string>>();

  function rememberRefs(repo: TrackedRepo, refs: RemoteRefs): void {
    const hints = new Map(refs.tags.map((t) => [t.sha, `refs/tags/${t.name}`]));
    hints.set(refs.head, `refs/heads/${refs.defaultBranch}`);
    refHints.set(fullName(repo), hints);
  }

  return {
    async listRefs(repo) {
      bare.newPass(repo);
      const args = ['-c', 'protocol.version=2', 'ls-remote', '--symref', '--end-of-options'];
      const out = await bare.runGit(
        [...args, repoUrl(repo), 'HEAD', 'refs/heads/*', 'refs/tags/*'],
        { timeoutMs: LIST_TIMEOUT_MS, label: 'git ls-remote' },
      );
      const refs = parseLsRemote(out);
      rememberRefs(repo, refs);
      return refs;
    },

    async commitInfo(repo, sha): Promise<CommitInfo> {
      const out = await bare.inRepo(
        repo,
        ['log', '-1', '--format=%cI%n%s', '--end-of-options', sha],
        'git log',
        LIST_TIMEOUT_MS,
      );
      const [date = '', subject = ''] = out.split('\n');
      return { date, subject };
    },

    materialize(repo, sha, destDir) {
      return bare.withRecovery(repo, async () => {
        await bare.ensureCommit(repo, sha, refHints.get(fullName(repo))?.get(sha));
        return extractArchive(bare, repo, sha, destDir);
      });
    },

    trimCache: (repo, maxBytes) => bare.trim(repo, maxBytes),
  };
}
