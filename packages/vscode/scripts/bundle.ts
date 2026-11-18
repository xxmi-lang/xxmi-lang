/** Bundles the extension (CommonJS, as VS Code loads it) into dist/. */
import { mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { build } from 'esbuild';

const root = resolve(import.meta.dirname, '..');
const dist = join(root, 'dist');
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

await build({
  entryPoints: [join(root, 'src/extension.ts')],
  outfile: join(dist, 'extension.cjs'),
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['vscode'],
  conditions: ['source'],
  minify: true,
  sourcemap: true,
});
console.log(`Bundled into ${dist}`);
