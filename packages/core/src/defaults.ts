/**
 * Where core finds its data when a caller doesn't pass it explicitly:
 *
 * - values set with `configureDefaults` (single-file binaries embed the spec and snapshots and
 *   set them at startup);
 * - in the repository, spec/ (generated + overlays) and packages/core/snapshots/;
 * - in a published package, dist/spec.json (written by the build) and snapshots/.
 */
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DEFAULT_SPEC_ROOT, loadSpec } from './spec/load.ts';
import type { Spec } from './spec/types.ts';
import type { SymbolSnapshot } from './workspace/symbols.ts';

const BUNDLED_SPEC = fileURLToPath(new URL('../dist/spec.json', import.meta.url));
const BUNDLED_SPEC_FROM_DIST = fileURLToPath(new URL('./spec.json', import.meta.url));
const SNAPSHOT_DIR = fileURLToPath(new URL('../snapshots/', import.meta.url));

let configuredSpec: Spec | undefined;
let configuredSnapshots: SymbolSnapshot[] | undefined;
let cachedSpec: Spec | undefined;

export function configureDefaults(options: { spec?: Spec; snapshots?: SymbolSnapshot[] }): void {
  if (options.spec) configuredSpec = options.spec;
  if (options.snapshots) configuredSnapshots = options.snapshots;
}

/** The merged spec (generated + overlays). */
export function defaultSpec(): Spec {
  if (configuredSpec) return configuredSpec;
  if (!cachedSpec) {
    if (existsSync(join(DEFAULT_SPEC_ROOT, 'generated', 'meta.json'))) cachedSpec = loadSpec();
    else {
      const bundled = [BUNDLED_SPEC_FROM_DIST, BUNDLED_SPEC].find((p) => existsSync(p));
      if (!bundled) throw new Error('No spec found: neither spec/ nor dist/spec.json is present.');
      cachedSpec = JSON.parse(readFileSync(bundled, 'utf8')) as Spec;
    }
  }
  return cachedSpec;
}

/** Library snapshots bundled with core (built by `xxmi index`). */
export function bundledSnapshots(): SymbolSnapshot[] {
  if (configuredSnapshots) return configuredSnapshots;
  let names: string[];
  try {
    names = readdirSync(SNAPSHOT_DIR)
      .filter((n) => n.endsWith('.json'))
      .sort();
  } catch {
    return [];
  }
  return names.map(
    (n) => JSON.parse(readFileSync(join(SNAPSHOT_DIR, n), 'utf8')) as SymbolSnapshot,
  );
}
