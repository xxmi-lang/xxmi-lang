import * as z from 'zod/v4';
import { latestPrerelease, latestRelease, type RepoState, type VersionRef } from '../catalog.ts';
import { fullName, TRACKED_REPOS, type TrackedRepo } from '../repos.ts';
import type { SpecStatus } from '../specs/registry.ts';
import { defineTool } from './define.ts';
import { resolveSpec, specLine, type ToolContext } from './shared.ts';

const summary = (v: VersionRef | undefined) =>
  v ? { name: v.name, sha: v.sha, publishedAt: v.publishedAt ?? null } : null;

type Summary = ReturnType<typeof summary>;

/** `latest release: v1.2.3 (2026-10-31)`; nothing when there is none. */
function latestLine(label: string, found: Summary): string[] {
  if (!found) return [];
  const date = found.publishedAt ? ` (${found.publishedAt.slice(0, 10)})` : '';
  return [`  latest ${label}: ${found.name}${date}`];
}

/** A DLL version with its spec status (`spec`, and `specError` when the build failed). */
async function withSpecStatus(ctx: ToolContext, version: VersionRef) {
  const status: SpecStatus = await ctx.specs.status(version.sha);
  return {
    ...version,
    spec: status.state,
    ...(status.error ? { specError: status.error } : {}),
  };
}

async function describeRepo(ctx: ToolContext, repo: TrackedRepo) {
  const state: RepoState | undefined = ctx.catalog.repoState(repo);
  const isDll = repo.role === 'dll';
  const decorate = async (v: VersionRef) => (isDll ? withSpecStatus(ctx, v) : v);
  return {
    repo: fullName(repo),
    alias: repo.alias,
    role: repo.role,
    indexed: state !== undefined,
    default: state ? { ...(await decorate(state.branch)), indexedAt: state.indexedAt } : null,
    tags: state ? await Promise.all(state.tags.map(decorate)) : [],
    latestRelease: state ? summary(latestRelease(state)) : null,
    latestPrerelease: state ? summary(latestPrerelease(state)) : null,
  };
}

type RepoDescription = Awaited<ReturnType<typeof describeRepo>>;

/** `spec: master ok, v1.3.2 ok, v1.2.3 failed (reason)`, for the DLL repo only. */
function specStatusLine(r: RepoDescription): string[] {
  if (r.role !== 'dll' || !r.default) return [];
  const versions = [r.default, ...r.tags] as { name: string; spec?: string; specError?: string }[];
  const parts = versions.map((v) => `${v.name} ${v.spec}${v.specError ? ` (${v.specError})` : ''}`);
  return [`  specs: ${parts.join(', ')}`];
}

export const listVersions = defineTool({
  name: 'list_versions',
  description:
    'Indexed repos and refs (default branch, releases, pre-releases), the spec status of each DLL version, the spec used by default, and the DLL commit the bundled spec was extracted from.',
  input: { format: z.enum(['text', 'json']).optional().describe('text (default) or json') },
  async run(ctx) {
    const repos = await Promise.all(TRACKED_REPOS.map((repo) => describeRepo(ctx, repo)));
    const { meta } = ctx.specs.bundled.spec;
    const spec = {
      dllCommit: meta.dll.commit,
      dllCommitDate: meta.dll.commitDate,
      baselineCommit: meta.baseline.commit,
    };
    const used = await resolveSpec(ctx, undefined);
    const defaultSpec = {
      source: used.source,
      dllSha: used.dllSha,
      tag: used.tag ?? null,
      kind: used.kind ?? null,
    };
    const lines = [
      `spec extracted from XXMI-Libs-Package ${spec.dllCommit.slice(0, 7)} (${spec.dllCommitDate})`,
      `default ${specLine(used)}`,
      ctx.catalog.ready
        ? 'index: ready'
        : 'index: warming up (spec tools work; library tools use the bundled snapshot)',
      ...repos.flatMap((r) =>
        r.default
          ? [
              `${r.repo}: ${r.default.name} @ ${r.default.sha.slice(0, 7)} (indexed ${r.default.indexedAt}); tags: ${r.tags.map((t) => t.name).join(', ') || 'none'}`,
              ...latestLine('release', r.latestRelease),
              ...latestLine('pre-release', r.latestPrerelease),
              ...specStatusLine(r),
            ]
          : [`${r.repo}: not indexed yet`],
      ),
    ];
    return { text: lines.join('\n'), data: { ready: ctx.catalog.ready, spec, defaultSpec, repos } };
  },
});
