/**
 * Launches a real VS Code (downloaded and cached by @vscode/test-electron) with the bundled
 * extension and runs test/suite.cjs inside it. Run `pnpm --filter ./packages/vscode build` first.
 * On Linux CI it needs a display (xvfb-run).
 */
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { runTests } from '@vscode/test-electron';

const root = resolve(import.meta.dirname, '..');
const fixture = resolve(root, '../../fixtures/lsp/package/Mods/MyMod');
try {
  await runTests({
    extensionDevelopmentPath: root,
    extensionTestsPath: join(root, 'test/suite.cjs'),
    launchArgs: [
      fixture,
      '--disable-extensions',
      '--user-data-dir',
      mkdtempSync(join(tmpdir(), 'xxmi-vscode-')),
    ],
  });
} catch (error) {
  console.error(error);
  process.exit(1);
}
