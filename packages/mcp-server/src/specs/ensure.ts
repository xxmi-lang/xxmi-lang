/** Builds the specs the registry is missing, one at a time, for the indexer's pass. */
import type { Catalog, RepoState, VersionRef } from '../catalog.ts';
import { errorMessage, type Logger } from '../logger.ts';
import { baselineRepo, dllRepo, fullName, type TrackedRepo } from '../repos.ts';
import type { BuildInput, BuiltSpec, SourceInput } from './build.ts';
import type { SpecRegistry } from './registry.ts';
import type { SpecHeader, StoredSpec } from './store.ts';

export type SpecBuilder = (input: BuildInput) => Promise<BuiltSpec>;

export interface EnsureDeps {
  registry: SpecRegistry;
  catalog: Catalog;
  repos: readonly TrackedRepo[];
  build: SpecBuilder;
  logger: Logger;
}

const MAX_WARNINGS = 50;
const MAX_ERROR_CHARS = 500;

/** A commit can be the head and a tag at once; it needs one spec. */
function distinctVersions(state: RepoState): VersionRef[] {
  const bySha = new Map<string, VersionRef>();
  for (const version of [state.branch, ...state.tags]) {
    if (!bySha.has(version.sha)) bySha.set(version.sha, version);
  }
  return [...bySha.values()];
}

function sourceOf(repo: TrackedRepo, version: VersionRef, dir: string): SourceInput {
  return {
    dir,
    commit: version.sha,
    commitDate: version.committedAt ?? '',
    repository: `https://github.com/${fullName(repo)}`,
  };
}

/** Builds the specs of every indexed DLL version the registry has no current entry for. */
export async function ensureSpecs(deps: EnsureDeps): Promise<string[]> {
  const dll = dllRepo(deps.repos);
  const baseline = baselineRepo(deps.repos);
  const dllState = dll && deps.catalog.repoState(dll);
  const baselineState = baseline && deps.catalog.repoState(baseline);
  // Without the baseline the xxmi flags can't be set: try again on the next pass.
  if (!dll || !baseline || !dllState || !baselineState) return [];

  const baselineInput = sourceOf(
    baseline,
    baselineState.branch,
    deps.catalog.resolve(baseline).dir,
  );
  const built: string[] = [];
  for (const version of distinctVersions(dllState)) {
    if (await deps.registry.slot(version.sha)) continue;
    const dllInput = sourceOf(dll, version, deps.catalog.resolve(dll, version.sha).dir);
    await buildOne(deps, { dll: dllInput, baseline: baselineInput });
    built.push(version.sha);
  }
  return built;
}

/** Records the outcome either way: a failed build is not retried until its inputs change. */
async function buildOne(deps: EnsureDeps, input: BuildInput): Promise<void> {
  const started = Date.now();
  const header: Omit<SpecHeader, 'status' | 'warnings'> = {
    dllSha: input.dll.commit,
    baselineSha: input.baseline.commit,
    builder: deps.registry.builder,
    builtAt: new Date().toISOString(),
  };
  let stored: StoredSpec;
  try {
    const { spec, warnings } = await deps.build(input);
    stored = { ...header, status: 'ok', warnings: warnings.slice(0, MAX_WARNINGS), spec };
  } catch (error) {
    stored = {
      ...header,
      status: 'failed',
      error: errorMessage(error).slice(0, MAX_ERROR_CHARS),
      warnings: [],
    };
  }

  try {
    await deps.registry.record(stored);
    deps.logger.info('spec built', {
      dll: input.dll.commit,
      status: stored.status,
      ms: Date.now() - started,
      ...(stored.error ? { error: stored.error } : {}),
    });
  } catch (error) {
    deps.logger.warn('spec not saved', { dll: input.dll.commit, error: errorMessage(error) });
  }
}
