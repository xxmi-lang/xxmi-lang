import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { externalServer } from '../src/external-server.ts';

let dir: string;
let pathVar: string;

beforeAll(() => {
  dir = mkdtempSync(join(tmpdir(), 'xxmi-which-'));
  writeFileSync(join(dir, 'xxmi-lsp'), '');
  chmodSync(join(dir, 'xxmi-lsp'), 0o755);
  writeFileSync(join(dir, 'xxmi-lsp.exe'), '');
  pathVar = ['/nonexistent', dir].join(delimiter);
});

afterAll(() => {
  rmSync(dir, { recursive: true, force: true });
});

describe('externalServer', () => {
  it('prefers the custom path over PATH', () => {
    expect(externalServer('/opt/xxmi', true, pathVar)).toEqual({
      command: '/opt/xxmi',
      args: ['--stdio'],
    });
  });

  it('uses xxmi-lsp from PATH only when preferPath is on', () => {
    expect(externalServer('', true, pathVar, 'linux')).toEqual({
      command: join(dir, 'xxmi-lsp'),
      args: ['--stdio'],
    });
    expect(externalServer('', false, pathVar, 'linux')).toBeUndefined();
  });

  it('looks for xxmi-lsp.exe on Windows', () => {
    expect(externalServer('', true, pathVar, 'win32')?.command).toBe(join(dir, 'xxmi-lsp.exe'));
  });

  it('falls back to the bundled server when PATH has none', () => {
    expect(externalServer('', true, '/nonexistent', 'linux')).toBeUndefined();
  });
});
