import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { describe, expect, it } from 'vitest';
import { buildInWorker } from '../src/specs/runner.ts';
import { specTrees } from './spec-fixtures.ts';
import { tempDir } from './helpers.ts';

const input = () => {
  const trees = specTrees();
  return {
    dll: { dir: trees.older, commit: 'a'.repeat(40) },
    baseline: { dir: trees.vanilla, commit: 'b'.repeat(40) },
  };
};

describe('buildInWorker', () => {
  it('builds the spec in a worker thread', async () => {
    const { spec } = await buildInWorker(input());

    expect(spec.commands.map((c) => c.name)).toContain('store');
  });

  it('reports a failing build with its message', async () => {
    const bad = input();
    bad.dll.dir = bad.baseline.dir.replace('vanilla', 'missing');

    await expect(buildInWorker(bad)).rejects.toThrow();
  });

  it('terminates a worker that outlives the timeout and reports a failure', async () => {
    const script = join(tempDir(), 'hang.mjs');
    writeFileSync(script, 'while (true) {}\n');

    const started = Date.now();
    await expect(
      buildInWorker(input(), { timeoutMs: 200, workerUrl: pathToFileURL(script) }),
    ).rejects.toThrow(/timed out after 200 ms/);

    expect(Date.now() - started).toBeLessThan(5_000);
  });
});
