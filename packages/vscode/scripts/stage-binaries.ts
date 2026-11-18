/**
 * Copies the release binaries (dist/bin/xxmi-<os>-<arch>[.exe], from `pnpm build:binaries`) into
 * packages/vscode/bin/<os>-<arch>/. Stages whichever exist; fails only when none do, because a
 * local checkout usually has just the host build.
 */
import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { BUNDLED_ARCH, BUNDLED_OS, binaryName } from '../src/platform.ts';

const root = resolve(import.meta.dirname, '..');
const source = resolve(root, '../../dist/bin');
const target = join(root, 'bin');
rmSync(target, { recursive: true, force: true });

const staged: string[] = [];
for (const os of BUNDLED_OS) {
  for (const arch of BUNDLED_ARCH) {
    const exe = binaryName(os).replace('xxmi', '');
    const from = join(source, `xxmi-${os}-${arch}${exe}`);
    if (!existsSync(from)) continue;
    const dir = join(target, `${os}-${arch}`);
    mkdirSync(dir, { recursive: true });
    const to = join(dir, binaryName(os));
    copyFileSync(from, to);
    chmodSync(to, 0o755);
    staged.push(`${os}-${arch}`);
  }
}

if (staged.length === 0) {
  console.error(
    `No binaries in ${source}: run \`pnpm build:binaries\` (or \`--target host\`) first.`,
  );
  process.exit(1);
}
console.log(`Staged binaries: ${staged.join(', ')}`);
