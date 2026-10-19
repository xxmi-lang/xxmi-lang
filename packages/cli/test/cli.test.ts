import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { main } from '../src/cli.ts';

const RULES = resolve(import.meta.dirname, '../../../fixtures/rules');
const NAMESPACES = resolve(import.meta.dirname, '../../../fixtures/namespaces');
const schema = JSON.parse(
  readFileSync(resolve(import.meta.dirname, '../schema/lint-output.v1.json'), 'utf8'),
) as object;

async function run(...argv: string[]): Promise<{ code: number; out: string; err: string }> {
  const out: string[] = [];
  const err: string[] = [];
  vi.spyOn(console, 'log').mockImplementation((...a: unknown[]) => void out.push(a.join(' ')));
  vi.spyOn(console, 'error').mockImplementation((...a: unknown[]) => void err.push(a.join(' ')));
  const code = await main(argv);
  return { code, out: out.join('\n'), err: err.join('\n') };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('xxmi lint', () => {
  it('exits 1 on errors and prints path:line:col  severity  id  message', async () => {
    const { code, out } = await run('lint', join(RULES, 'XM201/bad.ini'), '--rules', 'XM201');
    expect(code).toBe(1);
    expect(out).toMatch(
      /XM201\/bad\.ini:3:7 {2}error {2}XM201 {2}Unresolved command list `CommandListMissing`\./,
    );
    expect(out).toMatch(/1 file: 3 errors, 0 warnings, 0 info, 0 hints$/);
  });

  it('exits 0 with only warnings, 1 once --max-warnings is exceeded', async () => {
    const path = join(RULES, 'XM203/bad.ini');
    expect((await run('lint', path, '--rules', 'XM203')).code).toBe(0);
    expect((await run('lint', path, '--rules', 'XM203', '--max-warnings', '1')).code).toBe(1);
    expect((await run('lint', path, '--rules', 'XM203', '--max-warnings', '2')).code).toBe(0);
  });

  it('writes JSON that validates against lint-output.v1.json', async () => {
    const validate = new Ajv2020({ allErrors: true, strict: true }).compile(schema);
    for (const target of [RULES, NAMESPACES]) {
      const { out } = await run('lint', target, '--format', 'json');
      const json: unknown = JSON.parse(out);
      expect(validate(json), JSON.stringify(validate.errors)).toBe(true);
    }
  });

  it('reports usage errors with exit code 2', async () => {
    expect((await run('lint', '--format', 'xml')).code).toBe(2);
    expect((await run('lint', '--rules', 'XM999')).code).toBe(2);
    expect((await run('lint', join(RULES, 'no-such-file.ini'))).code).toBe(2);
    expect((await run('lint', '--bogus')).code).toBe(2);
    expect((await run('frobnicate')).code).toBe(2);
    expect((await run()).code).toBe(2);
  });

  it('prints its version', async () => {
    const { code, out } = await run('--version');
    expect(code).toBe(0);
    expect(out).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it('lists the rules in --help', async () => {
    const { code, out } = await run('lint', '--help');
    expect(code).toBe(0);
    expect(out).toContain('XM206  error    Unresolved namespace');
  });
});

describe('xxmi lsp', () => {
  // The server itself is covered by packages/lsp/test/protocol.test.ts (XXMI_LSP_COMMAND runs
  // it against `xxmi lsp` in CI); here only the wiring that doesn't start a server.
  it('documents itself and is listed in the usage', async () => {
    const { code, out } = await run('lsp', '--help');
    expect(code).toBe(0);
    expect(out).toContain('xxmi lsp [--stdio]');
    expect((await run('--help')).out).toContain('xxmi lsp [--stdio]');
  });
});

describe('xxmi index', () => {
  it('writes a sorted snapshot of a package', async () => {
    const out = join(mkdtempSync(join(tmpdir(), 'xxmi-index-')), 'snap.json');
    const { code } = await run(
      'index',
      join(NAMESPACES, 'rule2-path'),
      '--out',
      out,
      '--name',
      'test',
    );
    expect(code).toBe(0);
    const snapshot = JSON.parse(readFileSync(out, 'utf8')) as {
      name: string;
      namespaces: string[];
    };
    expect(snapshot.name).toBe('test');
    // d3dx.ini is loaded, but `include_recursive` folders (Mods) are not indexed.
    expect(snapshot.namespaces).toEqual([]);
  });

  it('needs a package dir with d3dx.ini', async () => {
    expect((await run('index', RULES, '--out', join(tmpdir(), 'x.json'))).code).toBe(2);
  });
});

describe('xxmi lint --stdin', () => {
  const stdin = (text: string) => () => Promise.resolve(new TextEncoder().encode(text));

  async function runStdin(
    text: string,
    ...argv: string[]
  ): Promise<{ code: number; out: string; err: string }> {
    const out: string[] = [];
    const err: string[] = [];
    vi.spyOn(console, 'log').mockImplementation((...a: unknown[]) => void out.push(a.join(' ')));
    vi.spyOn(console, 'error').mockImplementation((...a: unknown[]) => void err.push(a.join(' ')));
    const code = await main(argv, stdin(text));
    return { code, out: out.join('\n'), err: err.join('\n') };
  }

  const BAD = readFileSync(join(RULES, 'XM201/bad.ini'), 'utf8');
  const GOOD = readFileSync(join(RULES, 'XM201/good.ini'), 'utf8');

  it('lints the text as the file named by --stdin-path (the file need not exist)', async () => {
    const path = join(RULES, 'XM201/no-such-file.ini');
    const { code, out } = await runStdin(
      BAD,
      'lint',
      '--stdin',
      '--stdin-path',
      path,
      '--rules',
      'XM201',
    );
    expect(code).toBe(1);
    expect(out).toMatch(/no-such-file\.ini:3:7 {2}error {2}XM201 {2}Unresolved command list/);
    expect(out).toMatch(/1 file: 3 errors/);
  });

  it('reports the same as linting the file on disk, and nothing for a clean text', async () => {
    const path = join(RULES, 'XM201/bad.ini');
    const disk = await run('lint', path, '--format', 'json');
    const piped = await runStdin(BAD, 'lint', '--stdin', '--stdin-path', path, '--format', 'json');
    expect(JSON.parse(piped.out)).toEqual(JSON.parse(disk.out));
    // The text wins over the disk contents.
    const clean = await runStdin(GOOD, 'lint', '--stdin', '--stdin-path', path, '--rules', 'XM201');
    expect(clean.code).toBe(0);
    expect(clean.out).toMatch(/1 file: 0 errors/);
  });

  it('writes JSON that validates against lint-output.v1.json', async () => {
    const validate = new Ajv2020({ allErrors: true, strict: true }).compile(schema);
    const { out } = await runStdin(
      BAD,
      'lint',
      '--stdin',
      '--stdin-path',
      join(RULES, 'XM201/bad.ini'),
      '--format',
      'json',
    );
    expect(validate(JSON.parse(out) as unknown), JSON.stringify(validate.errors)).toBe(true);
  });

  it('takes the namespace and package from --stdin-path', async () => {
    // The text names itself by its path-derived namespace: inside the package that is
    // `Mods\\Other\\new.ini` (relative to d3dx.ini's folder), outside it is just `new.ini`.
    const text =
      '[CommandListFoo]\n\n[CommandListCaller]\nrun = CommandList\\Mods\\Other\\new.ini\\Foo\n';
    const lintAt = async (path: string) =>
      await runStdin(text, 'lint', '--stdin', '--stdin-path', path, '--rules', 'XM201,XM206');
    const inPackage = await lintAt(join(NAMESPACES, 'rule2-path/Mods/Other/new.ini'));
    expect(inPackage.code).toBe(0);
    expect(inPackage.out).toMatch(/1 file: 0 errors/);
    const outside = await lintAt(join(NAMESPACES, 'rule3-qualified/new.ini'));
    expect(outside.code).toBe(1);
    expect(outside.out).toMatch(/XM206 {2}Unknown namespace `Mods\\Other\\new\.ini`/);
  });

  it('is a usage error without --stdin-path, with paths, or --stdin-path alone', async () => {
    expect((await runStdin(BAD, 'lint', '--stdin')).code).toBe(2);
    expect((await runStdin(BAD, 'lint', '--stdin', '--stdin-path', 'a.ini', RULES)).code).toBe(2);
    expect((await runStdin(BAD, 'lint', '--stdin-path', 'a.ini')).code).toBe(2);
  });

  it('strips a UTF-8 BOM and reports the encoding', async () => {
    const withBom = new Uint8Array([0xef, 0xbb, 0xbf, ...new TextEncoder().encode(GOOD)]);
    const out: string[] = [];
    vi.spyOn(console, 'log').mockImplementation((...a: unknown[]) => void out.push(a.join(' ')));
    const code = await main(
      ['lint', '--stdin', '--stdin-path', join(RULES, 'XM201/good.ini'), '--format', 'json'],
      () => Promise.resolve(withBom),
    );
    expect(code).toBe(0);
    expect(
      (JSON.parse(out.join('\n')) as { files: { encoding: string }[] }).files[0]?.encoding,
    ).toBe('utf8bom');
  });
});
