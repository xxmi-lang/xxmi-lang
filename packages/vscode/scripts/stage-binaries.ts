/**
 * Copies the release binaries (dist/bin/xxmi-<os>-<arch>[.exe], from `pnpm build:binaries`) into
 * packages/vscode/bin/<os>-<arch>/. Stages whichever exist; fails only when none do, because a
 * local checkout usually has just the host build.
 */
import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import {
  RELEASE_TARGETS,
  type ReleaseTarget,
  releaseBinaryName,
  targetName,
} from '../../../scripts/platforms.ts';
import { binaryName } from '../src/platform.ts';

const root = resolve(import.meta.dirname, '..');
const source = resolve(root, '../../dist/bin');
const target = join(root, 'bin');
rmSync(target, { recursive: true, force: true });

function stage(t: ReleaseTarget): boolean {
  const from = join(source, releaseBinaryName(t));
  if (!existsSync(from)) return false;

  const dir = join(target, targetName(t));
  mkdirSync(dir, { recursive: true });
  const to = join(dir, binaryName(t.os));
  copyFileSync(from, to);
  chmodSync(to, 0o755);
  return true;
}

const staged = RELEASE_TARGETS.filter(stage).map(targetName);

if (staged.length === 0) {
  console.error(
    `No binaries in ${source}: run \`pnpm build:binaries\` (or \`--target host\`) first.`,
  );
  process.exit(1);
}
console.log(`Staged binaries: ${staged.join(', ')}`);
