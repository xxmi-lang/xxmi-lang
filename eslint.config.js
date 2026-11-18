// @ts-check
import js from '@eslint/js';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  {
    ignores: [
      '**/dist/**',
      '**/node_modules/**',
      'corpus/**',
      'coverage/**',
      '.zzmi-package/**',
      '.dll/**',
      // The Rust workspace (docs/rust-port.md) has its own tooling; only bench scripts are JS.
      'rust/**',
      // VS Code downloaded by @vscode/test-electron (packages/vscode/.vscode-test).
      '**/.vscode-test/**',
      // Temporary git worktrees (ESLint does not read .gitignore).
      '.claude/worktrees/**',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: {
          allowDefaultProject: ['*.js', '*.ts'],
        },
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
    },
  },
  {
    files: ['**/*.js', '**/*.cjs'],
    ...tseslint.configs.disableTypeChecked,
  },
  {
    // CommonJS run by VS Code's extension host (packages/vscode/test/suite.cjs).
    files: ['**/*.cjs'],
    languageOptions: {
      sourceType: 'commonjs',
      globals: {
        require: 'readonly',
        exports: 'writable',
        module: 'writable',
        __dirname: 'readonly',
        setTimeout: 'readonly',
      },
    },
    rules: { '@typescript-eslint/no-require-imports': 'off' },
  },
);
