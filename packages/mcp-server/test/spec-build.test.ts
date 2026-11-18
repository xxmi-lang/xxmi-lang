import { describe, expect, it } from 'vitest';
import { buildVersionSpec, builderId, type BuildInput } from '../src/specs/build.ts';
import { specTrees } from './spec-fixtures.ts';

const DLL_SHA = 'a'.repeat(40);
const BASELINE_SHA = 'b'.repeat(40);

function inputFor(dir: string, vanilla: string): BuildInput {
  return {
    dll: {
      dir,
      commit: DLL_SHA,
      commitDate: '2026-01-02T00:00:00Z',
      repository: 'https://example.invalid/xxmi',
    },
    baseline: { dir: vanilla, commit: BASELINE_SHA, commitDate: '2026-01-01T00:00:00Z' },
  };
}

describe('buildVersionSpec', () => {
  const trees = specTrees();
  const input = inputFor(trees.older, trees.vanilla);

  it('extracts the spec and records the given commits', () => {
    const { spec } = buildVersionSpec(input, []);

    expect(spec.meta.dll).toEqual({
      repository: 'https://example.invalid/xxmi',
      commit: DLL_SHA,
      commitDate: '2026-01-02T00:00:00Z',
    });
    expect(spec.meta.baseline.commit).toBe(BASELINE_SHA);
    expect(spec.commands.map((c) => c.name)).toContain('store');
    expect(spec.sections.find((s) => s.name === 'Resource')?.keys.map((k) => k.name)).toContain(
      'color_space',
    );
  });

  it('flags what the baseline lacks as XXMI-only', () => {
    const { spec } = buildVersionSpec(input, []);

    expect(spec.commands.find((c) => c.name === 'store')?.xxmi).toBe(true);
    expect(spec.sections.find((s) => s.name === 'Resource')?.xxmi).toBe(false);
  });

  it('merges the overlay docs and reports overlay entries this DLL lacks', () => {
    const overlays = [
      {
        file: 'test.toml',
        overlay: {
          commands: { store: { doc: 'Overlay doc.' }, gone: { doc: 'Not in this DLL.' } },
        },
      },
    ];

    const { spec, warnings } = buildVersionSpec(input, overlays);

    expect(spec.commands.find((c) => c.name === 'store')?.doc).toBe('Overlay doc.');
    expect(warnings.filter((w) => w.startsWith('overlay: test.toml: commands.gone'))).toHaveLength(
      1,
    );
  });

  it('merges the overlay bundled with the server by default', () => {
    const { spec } = buildVersionSpec(input);

    expect(spec.commands.some((c) => c.doc !== undefined)).toBe(true);
  });

  it('fails on sources that are not a DLL', () => {
    expect(() =>
      buildVersionSpec(inputFor(trees.vanilla, trees.vanilla.replace('vanilla', 'nope'))),
    ).toThrow();
  });
});

describe('builderId', () => {
  it('changes with the overlay', () => {
    expect(builderId([])).not.toBe(builderId([{ file: 'a.toml', overlay: {} }]));
    expect(builderId([])).toBe(builderId([]));
  });
});
