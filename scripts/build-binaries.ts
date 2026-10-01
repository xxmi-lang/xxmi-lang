/**
 * Builds the single-file `xxmi` and `xxmi-lsp` binaries with Bun (docs/00-overview.md
 * "Binaries"). Bun cross-compiles, so one machine builds every target.
 *
 *   pnpm build:binaries [--target bun-linux-x64,…] [--out dist/bin]
 *
 * Default: every release target. `--target host` builds only for this machine.
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

export const TARGETS = [
  'bun-windows-x64',
  'bun-windows-arm64',
  'bun-linux-x64',
  'bun-linux-arm64',
  'bun-darwin-x64',
  'bun-darwin-arm64',
];

const REPO = resolve(import.meta.dirname, '..');
const { values } = parseArgs({ options: { target: { type: 'string' }, out: { type: 'string' } } });
const out = resolve(values.out ?? join(REPO, 'dist/bin'));
const targets = values.target === 'host' ? ['host'] : (values.target?.split(',') ?? TARGETS);

// The embedded spec is the merged spec/ (generated + overlays).
execFileSync(process.execPath, [join(REPO, 'packages/core/scripts/bundle-spec.ts')], {
  stdio: 'inherit',
});
mkdirSync(out, { recursive: true });

for (const target of targets) {
  for (const name of ['xxmi', 'xxmi-lsp']) {
    const suffix = target === 'host' ? '' : `-${target.replace(/^bun-/, '')}`;
    const windows =
      target.includes('windows') || (target === 'host' && process.platform === 'win32');
    const exe = windows ? '.exe' : '';
    const outfile = join(out, `${name}${suffix}${exe}`);
    const args = [
      'build',
      join(REPO, `scripts/bin/${name}.ts`),
      '--compile',
      '--minify',
      '--conditions=source',
      // Don't let a .env or bunfig.toml in the user's mod folder change the binary's behavior.
      '--no-compile-autoload-dotenv',
      '--no-compile-autoload-bunfig',
      '--outfile',
      outfile,
      ...(target === 'host' ? [] : [`--target=${target}`]),
    ];
    console.log(`bun ${args.join(' ')}`);
    execFileSync('bun', args, { stdio: 'inherit', cwd: REPO, shell: process.platform === 'win32' });
  }
}
