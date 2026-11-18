import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import { buildVersionSpec } from '../src/specs/build.ts';
import { tempDir } from './helpers.ts';

// Opt-in: builds with the server's own path from real DLL tags, as the server does per version.
// XXMI_DLL_SRC is a XXMI-Libs-Package git checkout, MIGOTO_DLL_SRC a 3Dmigoto one.
const DLL = process.env['XXMI_DLL_SRC'];
const MIGOTO = process.env['MIGOTO_DLL_SRC'];

const TAGS = ['v1.2.0', 'v1.2.1', 'v1.2.2', 'v1.2.3', 'v1.3.0', 'v1.3.1', 'v1.3.2'];

function git(repo: string, args: string[]): Buffer {
  return execFileSync('git', ['-C', repo, ...args], { maxBuffer: 1 << 30 });
}

function materialize(repo: string, tag: string): { dir: string; commit: string } {
  const dir = tempDir();
  const archive = git(repo, ['archive', tag]);
  execFileSync('tar', ['-x', '-C', dir], { input: archive });
  return { dir, commit: git(repo, ['rev-parse', tag]).toString().trim() };
}

// each case extracts a full spec from a git archive
describe.skipIf(!DLL || !MIGOTO)('buildVersionSpec on real DLL tags', { timeout: 60_000 }, () => {
  const baseline = { dir: MIGOTO ?? '', commit: 'b'.repeat(40) };

  for (const tag of TAGS) {
    it(`builds ${tag}; overlay entries it lacks are warnings`, () => {
      const dll = materialize(DLL ?? '', tag);

      const { spec, warnings } = buildVersionSpec({ dll, baseline });

      expect(spec.commands.length).toBeGreaterThan(0);
      expect(
        warnings
          .filter((w) => w.startsWith('overlay:'))
          .every((w) => w.includes('not in the generated spec')),
      ).toBe(true);
    });
  }

  it('merges every overlay entry into v1.3.2', () => {
    const { warnings } = buildVersionSpec({ dll: materialize(DLL ?? '', 'v1.3.2'), baseline });

    expect(warnings.filter((w) => w.startsWith('overlay:'))).toEqual([]);
  });

  it('skips the 1.3.2 names in v1.3.1', () => {
    const { warnings } = buildVersionSpec({ dll: materialize(DLL ?? '', 'v1.3.1'), baseline });

    expect(warnings.some((w) => w.includes('commands.snapshotinitialtextures'))).toBe(true);
  });
});
