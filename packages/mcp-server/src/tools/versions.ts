import * as z from 'zod/v4';
import { fullName, TRACKED_REPOS } from '../repos.ts';
import { defineTool } from './define.ts';

export const listVersions = defineTool({
  name: 'list_versions',
  description:
    'Indexed repos and refs (default branch and release tags), plus the DLL commit the spec was extracted from.',
  input: { format: z.enum(['text', 'json']).optional().describe('text (default) or json') },
  run(ctx) {
    const repos = TRACKED_REPOS.map((repo) => {
      const state = ctx.catalog.repoState(repo);
      return {
        repo: fullName(repo),
        alias: repo.alias,
        indexed: state !== undefined,
        default: state ? { ...state.branch, indexedAt: state.indexedAt } : null,
        tags: state?.tags ?? [],
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
      ...repos.map((r) =>
        r.default
          ? `${r.repo}: ${r.default.name} @ ${r.default.sha.slice(0, 7)} (indexed ${r.default.indexedAt}); tags: ${r.tags.map((t) => t.name).join(', ') || 'none'}`
          : `${r.repo}: not indexed yet`,
      ),
    ];
    return Promise.resolve({
      text: lines.join('\n'),
      data: { ready: ctx.catalog.ready, spec, repos },
    });
  },
});
