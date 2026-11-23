/**
 * The `editors/manifest.json` shipped in every release zip: tells installers (ZZMI-Hub) which
 * editor config files to copy where. The checked-in copy has no `version`; the zipped one does.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export interface EditorFile {
  editor: string;
  root: string;
  source?: string;
  url?: string;
  sha256?: string;
  target: string;
  mode?: 'copy' | 'merge-json';
}

export interface EditorsManifest {
  manifestVersion: number;
  version?: string;
  roots: Record<string, Record<string, string>>;
  files: EditorFile[];
}

export const MANIFEST_PATH = 'editors/manifest.json';

export function loadManifest(repo: string): EditorsManifest {
  return JSON.parse(readFileSync(join(repo, MANIFEST_PATH), 'utf8')) as EditorsManifest;
}

/** `version` goes right after `manifestVersion`, so it is the first thing a reader sees. */
export function versionedManifest(manifest: EditorsManifest, version: string): string {
  const rest = Object.entries(manifest).filter(
    ([key]) => key !== 'manifestVersion' && key !== 'version',
  );
  const out = { manifestVersion: manifest.manifestVersion, version, ...Object.fromEntries(rest) };
  return `${JSON.stringify(out, null, 2)}\n`;
}

/** Paths (relative to the repo root) of the files the zip must carry next to the manifest. */
export function shippedSources(manifest: EditorsManifest): string[] {
  return manifest.files.flatMap((f) => (f.source ? [f.source] : []));
}
