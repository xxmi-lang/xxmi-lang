import { afterEach, describe, expect, it } from 'vitest';
import type { App } from '../src/app.ts';
import { specLine } from '../src/tools/shared.ts';
import {
  defaultRemotes,
  indexedApp,
  inlineBuild,
  SHA_A,
  SHA_B,
  TAG_SHA,
  coldApp,
} from './helpers.ts';
import { NEWER_KEY, versionedFixtures } from './spec-fixtures.ts';

const apps: App[] = [];
afterEach(async () => {
  await Promise.all(apps.splice(0).map((a) => a.close()));
});

const hasNewerKey = (spec: { sections: { name: string; keys: { name: string }[] }[] }): boolean =>
  spec.sections.some((s) => s.name === 'Resource' && s.keys.some((k) => k.name === NEWER_KEY));

async function versioned(options: { buildSpec?: typeof inlineBuild; dropBaseline?: boolean } = {}) {
  const { remotes, dirs } = versionedFixtures();
  if (options.dropBaseline) delete remotes['migoto'];
  const { app } = await indexedApp({}, remotes, {
    dirs,
    app: options.buildSpec ? { buildSpec: options.buildSpec } : {},
  });
  apps.push(app);
  return app.tools.specs;
}

describe('SpecRegistry.resolve', () => {
  it('serves the latest release by default, and its own spec for each ref form', async () => {
    const specs = await versioned();

    const byDefault = await specs.resolve(undefined);
    expect(byDefault).toMatchObject({
      source: 'extracted',
      dllSha: SHA_B,
      name: 'v1.2.3',
      kind: 'release',
    });
    expect(hasNewerKey(byDefault.spec)).toBe(false);

    const refs: [string, string, string][] = [
      ['latest-release', SHA_B, 'release'],
      ['latest-prerelease', TAG_SHA, 'prerelease'],
      ['head', SHA_A, 'head'],
      ['latest', SHA_A, 'head'],
      ['v1.2.3', SHA_B, 'release'],
      ['v1.3.1', TAG_SHA, 'prerelease'],
      [SHA_B.slice(0, 7), SHA_B, 'release'],
    ];
    for (const [ref, sha, kind] of refs) {
      const resolved = await specs.resolve(ref);
      expect(resolved, ref).toMatchObject({ source: 'extracted', dllSha: sha, kind, dllRef: ref });
      expect(hasNewerKey(resolved.spec), ref).toBe(kind !== 'release');
    }
  });

  it('gives tags a tag name and the head none', async () => {
    const specs = await versioned();

    expect((await specs.resolve('v1.3.1')).tag).toBe('v1.3.1');
    expect((await specs.resolve('head')).tag).toBeUndefined();
  });

  it('serves the bundled spec for "bundled", without a note', async () => {
    const specs = await versioned();

    const resolved = await specs.resolve('bundled');

    expect(resolved.source).toBe('bundled');
    expect(resolved.dllSha).toBe(specs.bundled.spec.meta.dll.commit);
    expect(resolved.note).toBeUndefined();
  });

  it('rejects an unknown ref and lists the valid choices', async () => {
    const specs = await versioned();

    await expect(specs.resolve('v9.9.9')).rejects.toThrow(
      /Indexed: .*v1\.2\.3.*latest-prerelease/s,
    );
  });

  it('falls back to the bundled spec while a version has no spec yet', async () => {
    const specs = await versioned({ dropBaseline: true });

    const resolved = await specs.resolve('v1.3.1');

    expect(resolved.source).toBe('bundled');
    expect(resolved.note).toContain('Requested v1.3.1');
    expect(resolved.note).toContain('still building');
    expect(resolved.note).toContain(specs.bundled.spec.meta.dll.commit.slice(0, 7));
    expect(specLine(resolved)).toMatch(
      /^spec: bundled \(\w{7}\); requested v1\.3\.1 \(ccccccc\) is still building$/,
    );
  });

  it('falls back to the bundled spec, with the error, when a version failed to build', async () => {
    const specs = await versioned({ buildSpec: () => Promise.reject(new Error('anchor missing')) });

    const resolved = await specs.resolve('head');

    expect(resolved.source).toBe('bundled');
    expect(resolved.note).toContain('could not be built (anchor missing)');
    expect(await specs.status(SHA_A)).toEqual({ state: 'failed', error: 'anchor missing' });
  });

  it('falls back to the bundled spec while nothing is indexed', async () => {
    const app = await coldApp();
    apps.push(app);

    const resolved = await app.tools.specs.resolve('latest-release');

    expect(resolved.source).toBe('bundled');
    expect(resolved.note).toContain('still warming up');
  });

  it('falls back when no release is indexed but one was asked for', async () => {
    const { app } = await indexedApp({}, defaultRemotes());
    apps.push(app);

    const resolved = await app.tools.specs.resolve('latest-release');

    expect(resolved.source).toBe('bundled');
    expect(resolved.note).toContain('no such version is indexed');
  });
});
