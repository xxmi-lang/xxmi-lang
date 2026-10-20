/** Picks the package index a `game` / `ref` pair refers to, falling back to the bundled snapshot. */
import type { SymbolSnapshot } from '@xxmi-lang/core';
import type { ResolvedRef } from '../catalog.ts';
import type { LoadedPackageIndex } from '../packages.ts';
import { gameRepo, TRACKED_REPOS } from '../repos.ts';
import { ToolError, type ToolContext } from './shared.ts';

export interface PackageChoice {
  snapshot: SymbolSnapshot;
  /** Absent while only the bundled snapshot is available. */
  resolved?: ResolvedRef;
  loaded?: LoadedPackageIndex;
  /** Shown to the caller when the answer is degraded. */
  note?: string;
}

export function supportedGames(): string[] {
  return TRACKED_REPOS.flatMap((r) => (r.game ? [r.game] : []));
}

export async function choosePackage(
  ctx: ToolContext,
  game: string,
  ref: string | undefined,
  options: { needDocs: boolean },
): Promise<PackageChoice> {
  const repo = gameRepo(game);
  if (!repo) {
    throw new ToolError(`Unknown game "${game}". Supported: ${supportedGames().join(', ')}.`);
  }
  if (ctx.catalog.isReady(repo)) {
    const resolved = ctx.catalog.resolve(repo, ref);
    const loaded = await ctx.packages.get(resolved);
    return { snapshot: loaded.snapshot, resolved, loaded };
  }
  const bundled = ctx.bundled(repo.game ?? '');
  if (ref && ref !== 'latest') {
    throw new ToolError(`${repo.name} is still warming up, so "${ref}" is not available yet.`);
  }
  if (!bundled) throw new ToolError(`${repo.name} is still warming up. Try again in a minute.`);
  return {
    snapshot: bundled,
    note: `Index is warming up: answering from the snapshot bundled with the server (${bundled.source.commit.slice(0, 7)}).${options.needDocs ? ' Doc comments and usage are not available from it.' : ''}`,
  };
}
