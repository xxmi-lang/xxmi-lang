/** DLL source trees for spec tests: the extractor's fixtures, in two versions that differ by a key. */
import { cpSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import {
  defaultRemotes,
  FIXTURE_DIRS,
  SHA_A,
  SHA_B,
  TAG_SHA,
  tempDir,
  type FakeRemote,
  type FixtureDirs,
} from './helpers.ts';

export const SPEC_FIXTURES = resolve(import.meta.dirname, '../../../fixtures/spec-extractor');

/** The key only the newer DLL reads, in `[Resource]`. */
export const NEWER_KEY = 'newer_only';

const ANCHOR = 'custom_resource->override_width = GetIniInt(section_name, L"width", -1, NULL);';

export interface SpecTrees {
  older: string;
  newer: string;
  vanilla: string;
}

export function specTrees(): SpecTrees {
  const older = tempDir();
  const newer = tempDir();
  cpSync(join(SPEC_FIXTURES, 'xxmi'), older, { recursive: true });
  cpSync(join(SPEC_FIXTURES, 'xxmi'), newer, { recursive: true });

  const file = join(newer, 'DirectX11', 'IniHandler.cpp');
  const text = readFileSync(file, 'utf8');
  if (!text.includes(ANCHOR)) throw new Error('fixture changed: anchor line not found');
  const added = `custom_resource->override_height = GetIniInt(section_name, L"${NEWER_KEY}", -1, NULL);`;
  writeFileSync(file, text.replace(ANCHOR, `${ANCHOR}\n\t${added}`));
  return { older, newer, vanilla: join(SPEC_FIXTURES, 'vanilla') };
}

/**
 * Head and the pre-release v1.3.1 read the newer sources, the release v1.2.3 the older ones;
 * the baseline is the vanilla fixture.
 */
export function versionedFixtures(): { remotes: Record<string, FakeRemote>; dirs: FixtureDirs } {
  const trees = specTrees();
  const remotes = defaultRemotes();
  remotes['dll'] = {
    head: { name: 'master', sha: SHA_A },
    tags: [
      { name: 'v1.2.3', sha: SHA_B },
      { name: 'v1.3.1', sha: TAG_SHA },
    ],
    releases: [
      {
        tag: 'v1.2.3',
        name: null,
        prerelease: false,
        draft: false,
        publishedAt: '2026-10-31T09:00:00Z',
      },
      {
        tag: 'v1.3.1',
        name: null,
        prerelease: true,
        draft: false,
        publishedAt: '2026-11-15T09:00:00Z',
      },
    ],
  };
  const dirs: FixtureDirs = {
    ...FIXTURE_DIRS,
    [`dll@${SHA_A}`]: trees.newer,
    [`dll@${SHA_B}`]: trees.older,
    [`dll@${TAG_SHA}`]: trees.newer,
    migoto: trees.vanilla,
  };
  return { remotes, dirs };
}
