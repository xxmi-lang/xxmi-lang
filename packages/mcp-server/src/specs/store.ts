/** Built specs on the volume: `<DATA_DIR>/specs/<dllSha>.json`, one per indexed DLL commit. */
import { readdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { Spec } from '@xxmi-lang/core';
import type { Store } from '../store.ts';

export interface SpecHeader {
  dllSha: string;
  baselineSha: string;
  /** `builderId()` of the server that built it. */
  builder: string;
  builtAt: string;
  status: 'ok' | 'failed';
  error?: string;
  warnings: string[];
}

export interface StoredSpec extends SpecHeader {
  /** Present when `status` is `ok`. */
  spec?: Spec;
}

/** What a stored entry must match to be used instead of rebuilt. */
export interface Expected {
  dllSha: string;
  baselineSha: string;
  builder: string;
}

export function isCurrent(header: SpecHeader, expected: Expected): boolean {
  return (
    header.dllSha === expected.dllSha &&
    header.baselineSha === expected.baselineSha &&
    header.builder === expected.builder
  );
}

const SHA_FILE = /^([0-9a-f]{40})\.json$/;

export class SpecStore {
  private readonly store: Store;

  constructor(store: Store) {
    this.store = store;
  }

  private get dir(): string {
    return join(this.store.dataDir, 'specs');
  }

  private file(dllSha: string): string {
    return join(this.dir, `${dllSha}.json`);
  }

  read(dllSha: string): Promise<StoredSpec | undefined> {
    return this.store.readJson<StoredSpec>(this.file(dllSha));
  }

  write(entry: StoredSpec): Promise<void> {
    return this.store.writeJsonAtomic(this.file(entry.dllSha), entry);
  }

  /** Removes the specs of commits that are no longer indexed; returns their shas. */
  async prune(keep: ReadonlySet<string>): Promise<string[]> {
    const removed: string[] = [];
    for (const name of await readdir(this.dir).catch(() => [])) {
      const sha = SHA_FILE.exec(name)?.[1];
      if (!sha || keep.has(sha)) continue;
      await rm(join(this.dir, name), { force: true });
      removed.push(sha);
    }
    return removed;
  }
}
