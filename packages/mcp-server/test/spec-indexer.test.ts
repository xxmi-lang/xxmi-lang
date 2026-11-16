import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { createApp, type App } from '../src/app.ts';
import type { BuildInput } from '../src/specs/build.ts';
import type { SpecBuilder } from '../src/specs/ensure.ts';
import {
  fakeSources,
  inlineBuild,
  silentLogger,
  SHA_A,
  SHA_B,
  TAG_SHA,
  tempDir,
  testConfig,
  waitFor,
} from './helpers.ts';
import { versionedFixtures } from './spec-fixtures.ts';

const apps: App[] = [];
afterEach(async () => {
  await Promise.all(apps.splice(0).map((a) => a.close()));
});

/** An app on `dataDir` over the versioned fixtures; `builds` records every spec build. */
async function setup(
  options: {
    dataDir?: string;
    builder?: string;
    buildSpec?: SpecBuilder;
    dropBaseline?: boolean;
  } = {},
) {
  const dataDir = options.dataDir ?? tempDir();
  const { remotes, dirs } = versionedFixtures();
  if (options.dropBaseline) delete remotes['migoto'];
  const sources = fakeSources(remotes, dirs);
  const builds: BuildInput[] = [];
  const inner = options.buildSpec ?? inlineBuild;
  const app = await createApp(testConfig({ dataDir }), {
    sources,
    logger: silentLogger,
    builder: options.builder ?? 'builder-1',
    buildSpec: (input) => {
      builds.push(input);
      return inner(input);
    },
  });
  apps.push(app);
  return { app, builds, dataDir, sources, remotes };
}

const statuses = (app: App) =>
  Promise.all(
    [SHA_A, SHA_B, TAG_SHA].map(async (sha) => (await app.tools.specs.status(sha)).state),
  );

describe('spec builds in the indexer', () => {
  it('builds the spec of every indexed DLL version after indexing', async () => {
    const { app, builds, dataDir } = await setup();

    const result = await app.indexer.runOnce();

    expect(result.errors).toEqual([]);
    expect(await statuses(app)).toEqual(['ok', 'ok', 'ok']);
    expect(builds.map((b) => b.dll.commit).sort()).toEqual([SHA_A, SHA_B, TAG_SHA].sort());
    expect(builds.every((b) => b.baseline.commit === SHA_A)).toBe(true);
    expect(existsSync(join(dataDir, 'specs', `${SHA_B}.json`))).toBe(true);
  });

  it('does not rebuild what is stored and current', async () => {
    const { app, builds } = await setup();

    await app.indexer.runOnce();
    await app.indexer.runOnce();

    expect(builds).toHaveLength(3);
  });

  it('skips the builds while the baseline is missing and builds on the next pass', async () => {
    const { app, builds, sources } = await setup({ dropBaseline: true });

    await app.indexer.runOnce();
    expect(builds).toHaveLength(0);
    expect(await statuses(app)).toEqual(['pending', 'pending', 'pending']);

    sources.remotes['migoto'] = { head: { name: 'master', sha: SHA_A }, tags: [] };
    await app.indexer.runOnce();

    expect(builds).toHaveLength(3);
    expect(await statuses(app)).toEqual(['ok', 'ok', 'ok']);
  });

  it('does not fail the pass or readiness when a build throws, and keeps the failure', async () => {
    const { app, builds } = await setup({
      buildSpec: (input) =>
        input.dll.commit === SHA_B
          ? Promise.reject(new Error('anchor missing'))
          : inlineBuild(input),
    });

    const result = await app.indexer.runOnce();

    expect(result.errors).toEqual([]);
    expect(app.catalog.ready).toBe(true);
    expect(await statuses(app)).toEqual(['ok', 'failed', 'ok']);
    await app.indexer.runOnce();
    expect(builds).toHaveLength(3);
  });

  it('does not wait for the builds to report ready', async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { app } = await setup({ buildSpec: async (input) => (await gate, inlineBuild(input)) });

    const pass = app.indexer.runOnce();
    await waitFor(() => app.catalog.ready);

    expect(app.catalog.ready).toBe(true);
    release();
    await pass;
  });

  it('rebuilds when the server version changes, and only then', async () => {
    const first = await setup();
    await first.app.indexer.runOnce();
    await first.app.close();

    const same = await setup({ dataDir: first.dataDir });
    await same.app.indexer.runOnce();
    expect(same.builds).toHaveLength(0);
    expect(await statuses(same.app)).toEqual(['ok', 'ok', 'ok']);
    await same.app.close();

    const upgraded = await setup({ dataDir: first.dataDir, builder: 'builder-2' });
    expect(await statuses(upgraded.app)).toEqual(['pending', 'pending', 'pending']);
    await upgraded.app.indexer.runOnce();
    expect(upgraded.builds).toHaveLength(3);
  });

  it('retries a failed build once the server version changes', async () => {
    const failing = await setup({ buildSpec: () => Promise.reject(new Error('boom')) });
    await failing.app.indexer.runOnce();
    await failing.app.close();

    const same = await setup({ dataDir: failing.dataDir, builder: 'builder-1' });
    await same.app.indexer.runOnce();
    expect(same.builds).toHaveLength(0);
    await same.app.close();

    const upgraded = await setup({ dataDir: failing.dataDir, builder: 'builder-2' });
    await upgraded.app.indexer.runOnce();
    expect(await statuses(upgraded.app)).toEqual(['ok', 'ok', 'ok']);
  });

  it('removes the spec of a version that is no longer indexed', async () => {
    const { app, dataDir, sources } = await setup();
    await app.indexer.runOnce();
    expect(existsSync(join(dataDir, 'specs', `${SHA_B}.json`))).toBe(true);

    const dll = sources.remotes['dll'];
    if (!dll) throw new Error('fixture has no dll remote');
    sources.remotes['dll'] = { ...dll, tags: [{ name: 'v1.3.1', sha: TAG_SHA }] };
    await app.indexer.runOnce();
    await app.indexer.runOnce();

    expect(existsSync(join(dataDir, 'specs', `${SHA_B}.json`))).toBe(false);
    expect(existsSync(join(dataDir, 'specs', `${TAG_SHA}.json`))).toBe(true);
  });
});
