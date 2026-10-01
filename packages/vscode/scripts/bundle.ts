/**
 * Bundles the extension (CommonJS, as VS Code loads it) and the server (ESM, run by VS Code's
 * Node with --liftoff-only), and copies the server's data files into dist/.
 */
import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { build } from 'esbuild';

const root = resolve(import.meta.dirname, '..');
const core = resolve(root, '../core');
const dist = join(root, 'dist');
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

execFileSync(process.execPath, [join(core, 'scripts/bundle-spec.ts')], { stdio: 'inherit' });

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

await build({
  entryPoints: [join(root, 'src/server.ts')],
  outfile: join(dist, 'server.mjs'),
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node20',
  conditions: ['source'],
  minify: true,
  sourcemap: true,
  // CommonJS dependencies (vscode-languageserver) call require() for Node builtins.
  banner: {
    js: "import { createRequire as __xxmiCreateRequire } from 'node:module'; const require = __xxmiCreateRequire(import.meta.url);",
  },
});

for (const [from, name] of [
  [join(core, 'dist/spec.json'), 'spec.json'],
  [join(core, 'snapshots/zzmi.json'), 'zzmi.json'],
  [join(core, 'grammar/tree-sitter-migoto.wasm'), 'tree-sitter-migoto.wasm'],
  [join(core, 'node_modules/web-tree-sitter/web-tree-sitter.wasm'), 'web-tree-sitter.wasm'],
] as const) {
  copyFileSync(from, join(dist, name));
}
console.log(`Bundled into ${dist}`);
