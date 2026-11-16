import { existsSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { isCurrent, SpecStore, type StoredSpec } from '../src/specs/store.ts';
import { Store } from '../src/store.ts';
import { SHA_A, SHA_B, tempDir } from './helpers.ts';

const expected = { dllSha: SHA_A, baselineSha: 'f'.repeat(40), builder: '1.0+x' };

const entry = (overrides: Partial<StoredSpec> = {}): StoredSpec => ({
  ...expected,
  builtAt: '2026-11-16T00:00:00Z',
  status: 'ok',
  warnings: [],
  ...overrides,
});

async function setup() {
  const dataDir = tempDir();
  const store = new Store(dataDir);
  await store.init();
  return { dataDir, specs: new SpecStore(store) };
}

describe('isCurrent', () => {
  it('needs the same commit, baseline and builder', () => {
    expect(isCurrent(entry(), expected)).toBe(true);
    expect(isCurrent(entry({ builder: '2.0+x' }), expected)).toBe(false);
    expect(isCurrent(entry({ baselineSha: 'e'.repeat(40) }), expected)).toBe(false);
    expect(isCurrent(entry({ dllSha: SHA_B }), expected)).toBe(false);
  });

  it('treats a failed entry like any other, so it is not retried until an input changes', () => {
    const failed = entry({ status: 'failed', error: 'boom' });

    expect(isCurrent(failed, expected)).toBe(true);
    expect(isCurrent(failed, { ...expected, builder: '2.0+x' })).toBe(false);
  });
});

describe('SpecStore', () => {
  it('round-trips an entry under specs/<sha>.json', async () => {
    const { dataDir, specs } = await setup();

    await specs.write(entry({ status: 'failed', error: 'boom' }));

    expect(existsSync(join(dataDir, 'specs', `${SHA_A}.json`))).toBe(true);
    expect(await specs.read(SHA_A)).toMatchObject({ status: 'failed', error: 'boom' });
    expect(await specs.read(SHA_B)).toBeUndefined();
  });

  it('writes through a rename and leaves no staging file behind', async () => {
    const { dataDir, specs } = await setup();

    await specs.write(entry());
    await specs.write(entry({ builtAt: '2026-11-17T00:00:00Z' }));

    expect(readdirSync(join(dataDir, 'tmp'))).toEqual([]);
    expect((await specs.read(SHA_A))?.builtAt).toBe('2026-11-17T00:00:00Z');
  });

  it('treats an unreadable file as missing', async () => {
    const { dataDir, specs } = await setup();
    await specs.write(entry());
    writeFileSync(join(dataDir, 'specs', `${SHA_A}.json`), '{ cut off');

    expect(await specs.read(SHA_A)).toBeUndefined();
  });

  it('prunes the specs of commits that are not kept, and only those', async () => {
    const { specs } = await setup();
    await specs.write(entry());
    await specs.write(entry({ dllSha: SHA_B }));

    const removed = await specs.prune(new Set([SHA_A]));

    expect(removed).toEqual([SHA_B]);
    expect(await specs.read(SHA_A)).toBeDefined();
    expect(await specs.read(SHA_B)).toBeUndefined();
  });

  it('prunes nothing before the first spec was written', async () => {
    const { specs } = await setup();

    expect(await specs.prune(new Set())).toEqual([]);
  });
});
