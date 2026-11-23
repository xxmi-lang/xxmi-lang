import { readdirSync } from 'node:fs';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    projects: readdirSync('packages').map((name) => ({
      extends: true,
      test: {
        name: `@xxmi-lang/${name}`,
        root: `packages/${name}`,
      },
    })),
  },
});
