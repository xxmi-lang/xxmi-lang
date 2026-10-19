import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { VERSION } from '../src/version.ts';

const readJson = (relative: string): { version: string } =>
  JSON.parse(readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8')) as {
    version: string;
  };

describe('release version', () => {
  it('is the same in core, the VS Code extension and the Claude Code plugin', () => {
    expect(readJson('../../vscode/package.json').version).toBe(VERSION);
    expect(readJson('../../../plugin/.claude-plugin/plugin.json').version).toBe(VERSION);
  });
});
