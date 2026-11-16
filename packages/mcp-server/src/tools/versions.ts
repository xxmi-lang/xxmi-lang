import * as z from 'zod/v4';
import { latestPrerelease, latestRelease, type VersionRef } from '../catalog.ts';
import { fullName, TRACKED_REPOS } from '../repos.ts';
import { defineTool } from './define.ts';

const summary = (v: VersionRef | undefined) =>
  v ? { name: v.name, sha: v.sha, publishedAt: v.publishedAt ?? null } : null;

type Summary = ReturnType<typeof summary>;

/** `latest release: v1.2.3 (2026-10-31)`; nothing when there is none. */
function latestLine(label: string, found: Summary): string[] {
  if (!found) return [];
  const date = found.publishedAt ? ` (${found.publishedAt.slice(0, 10)})` : '';
  return [`  latest ${label}: ${found.name}${date}`];
}

export const listVersions = defineTool({
  name: 'list_versions',
  description:
    'Indexed repos and refs (default branch, releases, pre-releases), plus the DLL commit the spec was extracted from.',
  input: { format: z.enum(['text', 'json']).optional().describe('text (default) or json') },
  run(ctx) {
    const repos = TRACKED_REPOS.map((repo) => {
      const state = ctx.catalog.repoState(repo);
      return {
        repo: fullName(repo),
        alias: repo.alias,
        role: repo.role,
        indexed: state !== undefined,
        default: state ? { ...state.branch, indexedAt: state.indexedAt } : null,
        tags: state?.tags ?? [],
        latestRelease: state ? summary(latestRelease(state)) : null,
        latestPrerelease: state ? summary(latestPrerelease(state)) : null,
      };
    });
    const spec = {
      dllCommit: ctx.spec.meta.dll.commit,
      dllCommitDate: ctx.spec.meta.dll.commitDate,
      baselineCommit: ctx.spec.meta.baseline.commit,
    };
    const lines = [
      `spec extracted from XXMI-Libs-Package ${spec.dllCommit.slice(0, 7)} (${spec.dllCommitDate})`,
      ctx.catalog.ready
        ? 'index: ready'
        : 'index: warming up (spec tools work; library tools use the bundled snapshot)',
      ...repos.flatMap((r) =>
        r.default
          ? [
              `${r.repo}: ${r.default.name} @ ${r.default.sha.slice(0, 7)} (indexed ${r.default.indexedAt}); tags: ${r.tags.map((t) => t.name).join(', ') || 'none'}`,
              ...latestLine('release', r.latestRelease),
              ...latestLine('pre-release', r.latestPrerelease),
            ]
          : [`${r.repo}: not indexed yet`],
      ),
    ];
    return Promise.resolve({
      text: lines.join('\n'),
      data: { ready: ctx.catalog.ready, spec, repos },
    });
  },
});
