import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  loadManifest,
  shippedSources,
  versionedManifest,
} from '../../../scripts/editors-manifest.ts';

const REPO = resolve(import.meta.dirname, '../../..');
const manifest = loadManifest(REPO);

describe('editors manifest', () => {
  it('has no version checked in', () => {
    expect(manifest.version).toBeUndefined();
  });

  it('ships sources that exist', () => {
    for (const source of shippedSources(manifest)) {
      expect(existsSync(join(REPO, source)), source).toBe(true);
    }
  });

  it('gives every file either a source or a pinned url', () => {
    for (const f of manifest.files) {
      const hasSource = f.source !== undefined;
      const hasUrl = f.url !== undefined && /^[0-9a-f]{64}$/.test(f.sha256 ?? '');
      expect(hasSource !== hasUrl, f.target).toBe(true);
      expect(hasSource && (f.url !== undefined || f.sha256 !== undefined), f.target).toBe(false);
    }
  });

  it('only uses declared roots', () => {
    for (const f of manifest.files) expect(manifest.roots).toHaveProperty(f.root);
  });

  it('inserts the version right after manifestVersion', () => {
    const out = JSON.parse(versionedManifest(manifest, '1.2.3')) as Record<string, unknown>;
    expect(Object.keys(out).slice(0, 2)).toEqual(['manifestVersion', 'version']);
    expect(out.version).toBe('1.2.3');
    expect(out.files).toEqual(manifest.files);
  });
});
