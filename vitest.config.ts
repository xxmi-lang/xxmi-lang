import { readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { defineConfig } from 'vitest/config';

// Workspace packages resolve to their TypeScript sources, never to a possibly stale dist/ build
// (the same thing the `source` export condition does for tsc and `node --conditions=source`).
export default defineConfig({
  resolve: {
    alias: { '@xxmi-lang/core': resolve(import.meta.dirname, 'packages/core/src/index.ts') },
  },
  test: {
    projects: readdirSync('packages').map((name) => ({
      extends: true,
      test: { name: `@xxmi-lang/${name}`, root: `packages/${name}` },
    })),
  },
});
