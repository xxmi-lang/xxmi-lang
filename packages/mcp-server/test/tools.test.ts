import { afterEach, describe, expect, it } from 'vitest';
import type { App } from '../src/app.ts';
import { callTool, coldApp, connectClient, indexedApp } from './helpers.ts';

const apps: App[] = [];
afterEach(async () => {
  await Promise.all(apps.splice(0).map((a) => a.close()));
});

async function client() {
  const { app } = await indexedApp();
  apps.push(app);
  return connectClient(app);
}

describe('tools/list contract', () => {
  it('keeps tool names and input schemas stable', async () => {
    const tools = (await (await client()).listTools()).tools;

    const contract = tools.map((t) => ({
      name: t.name,
      required: (t.inputSchema as { required?: string[] }).required ?? [],
      properties: Object.keys((t.inputSchema as { properties: object }).properties).sort(),
    }));

    expect(contract).toMatchInlineSnapshot(`
      [
        {
          "name": "spec_lookup",
          "properties": [
            "dll_ref",
            "format",
            "kind",
            "name",
            "section",
          ],
          "required": [
            "name",
          ],
        },
        {
          "name": "spec_list",
          "properties": [
            "cursor",
            "format",
            "kind",
            "section",
          ],
          "required": [
            "kind",
          ],
        },
        {
          "name": "library_list",
          "properties": [
            "cursor",
            "format",
            "game",
            "ref",
          ],
          "required": [
            "game",
          ],
        },
        {
          "name": "library_symbol",
          "properties": [
            "format",
            "game",
            "qualified_name",
            "ref",
          ],
          "required": [
            "qualified_name",
            "game",
          ],
        },
        {
          "name": "find_implementation",
          "properties": [
            "feature",
            "format",
            "limit",
            "ref",
          ],
          "required": [
            "feature",
          ],
        },
        {
          "name": "search_source",
          "properties": [
            "format",
            "max_results",
            "path_glob",
            "query",
            "ref",
            "repo",
          ],
          "required": [
            "repo",
            "query",
          ],
        },
        {
          "name": "read_source",
          "properties": [
            "end_line",
            "format",
            "path",
            "ref",
            "repo",
            "start_line",
          ],
          "required": [
            "repo",
            "path",
            "start_line",
            "end_line",
          ],
        },
        {
          "name": "lint_ini",
          "properties": [
            "files",
            "format",
            "game",
            "package_ref",
          ],
          "required": [
            "files",
          ],
        },
        {
          "name": "list_versions",
          "properties": [
            "format",
          ],
          "required": [],
        },
      ]
    `);
  });
});

describe('spec_lookup / spec_list', () => {
  it('answers with docs, the XXMI flag and the DLL location', async () => {
    const { text, isError } = await callTool(await client(), 'spec_lookup', { name: 'store' });

    expect(isError).toBe(false);
    expect(text).toContain('command store  [XXMI-only]');
    expect(text).toContain('DirectX11/CommandList.cpp:');
    expect(text).toContain('github.com/SpectrumQT/XXMI-Libs-Package/blob/');
  });

  it('matches prefixed section names and narrows keys by section', async () => {
    const c = await client();

    expect((await callTool(c, 'spec_lookup', { name: 'ResourceFoo' })).text).toContain(
      'section Resource',
    );
    const key = await callTool(c, 'spec_lookup', {
      name: 'hash',
      kind: 'key',
      section: 'TextureOverride',
    });
    expect(key.text).toContain('in [TextureOverride]');
  });

  it('says so when nothing matches, and rejects other spec versions', async () => {
    const c = await client();

    expect((await callTool(c, 'spec_lookup', { name: 'nonsense' })).text).toContain(
      'No spec entry',
    );
    const old = await callTool(c, 'spec_lookup', { name: 'store', dll_ref: 'deadbeef1' });
    expect(old.isError).toBe(true);
  });

  it('lists the keys of a section and paginates', async () => {
    const c = await client();

    const keys = await callTool(c, 'spec_list', { kind: 'key', section: 'Resource' });
    expect(keys.text).toMatch(/^filename/m);
    const all = await callTool(c, 'spec_list', { kind: 'enum', format: 'json' });
    expect((JSON.parse(all.text) as { items: unknown[] }).items.length).toBeGreaterThan(0);
    expect((await callTool(c, 'spec_list', { kind: 'key' })).isError).toBe(true);
    expect((await callTool(c, 'spec_list', { kind: 'command', cursor: '!!' })).isError).toBe(true);
  });
});

describe('library tools', () => {
  it('describes a symbol with doc tags, usage and namespace members', async () => {
    const { text } = await callTool(await client(), 'library_symbol', {
      qualified_name: 'CommandList\\Demo\\SetThings',
      game: 'zzmi',
    });

    expect(text).toContain('declared: Core/Lib/Lib.ini:');
    expect(text).toContain('github.com/leotorrez/ZZMI-Package/blob/');
    expect(text).toContain('/ZZMI/Core/Lib/Lib.ini#L');
    expect(text).toContain('input: Resource\\Demo\\Diffuse diffuse texture (required)');
    expect(text).toContain('resource Resource\\Demo\\Normal');
    expect(text).toContain('Usage:');
  });

  it('describes a global variable and suggests names for a miss', async () => {
    const c = await client();

    const variable = await callTool(c, 'library_symbol', {
      qualified_name: '$\\Demo\\skip_glow',
      game: 'zzmi',
    });
    expect(variable.text).toContain('Set to 1 to skip the glow pass.');
    const miss = await callTool(c, 'library_symbol', {
      qualified_name: 'CommandList\\Demo\\SetThing',
      game: 'zzmi',
    });
    expect(miss.isError).toBe(true);
    expect(miss.text).toContain('Similar:');
  });

  it('lists libraries with their module docs, for a known tag but not an unknown ref', async () => {
    const c = await client();

    expect((await callTool(c, 'library_list', { game: 'zzmi' })).text).toContain('Demo');
    expect((await callTool(c, 'library_list', { game: 'zzmi', ref: 'v1.0.0' })).isError).toBe(
      false,
    );
    const bad = await callTool(c, 'library_list', { game: 'zzmi', ref: 'v9' });
    expect(bad.isError).toBe(true);
    expect(bad.text).toContain('Indexed: main, v1.0.0');
    expect((await callTool(c, 'library_list', { game: 'nope' })).isError).toBe(true);
  });

  it('answers from the bundled snapshot while warming up', async () => {
    const app = await coldApp();
    apps.push(app);
    const c = await connectClient(app);

    const symbol = await callTool(c, 'library_symbol', {
      qualified_name: 'CommandList\\ZZMI\\SetTextures',
      game: 'zzmi',
    });

    expect(symbol.isError).toBe(false);
    expect(symbol.text).toContain('warming up');
    expect(symbol.text).toContain('Resources.ini');
    expect((await callTool(c, 'find_implementation', { feature: 'store' })).text).toContain(
      'warming up',
    );
  });
});

describe('DLL and source tools', () => {
  it('finds where store is implemented', async () => {
    const { text } = await callTool(await client(), 'find_implementation', { feature: 'store' });

    expect(text).toContain('DirectX11/CommandList.cpp:5  literal in ParseStoreCommand');
    expect(text).toContain(
      'DirectX11/CommandList.cpp:11  literal in ParseCommandListGeneralCommands',
    );
  });

  it('reads an excerpt with line numbers and refuses paths outside the repo or too many lines', async () => {
    const c = await client();

    const ok = await callTool(c, 'read_source', {
      repo: 'dll',
      path: 'DirectX11/CommandList.cpp',
      start_line: 3,
      end_line: 5,
    });
    expect(ok.text).toContain('4| {');
    expect(ok.text).toContain('5| \tCommandArgumentReader args(L"store"');
    for (const path of ['../secret', '/etc/passwd', 'C:\\Windows\\win.ini', 'a/../../x']) {
      const bad = await callTool(c, 'read_source', {
        repo: 'dll',
        path,
        start_line: 1,
        end_line: 2,
      });
      expect(bad.isError, path).toBe(true);
    }
    const long = await callTool(c, 'read_source', {
      repo: 'dll',
      path: 'DirectX11/CommandList.cpp',
      start_line: 1,
      end_line: 500,
    });
    expect(long.text).toContain('at most 400 lines');
    expect(
      (
        await callTool(c, 'read_source', {
          repo: 'someone/else',
          path: 'a',
          start_line: 1,
          end_line: 2,
        })
      ).isError,
    ).toBe(true);
  });

  it('lists versions', async () => {
    const { text } = await callTool(await client(), 'list_versions', {});

    expect(text).toContain('index: ready');
    expect(text).toContain('leotorrez/ZZMI-Package: main');
  });
});

describe('lint_ini', () => {
  it('reports diagnostics in the xxmi lint json shape', async () => {
    const result = await callTool(await client(), 'lint_ini', {
      files: { 'Mods/A/mod.ini': '[TextureOverrideA]\nhash = 11111111\nrun = CommandListLocl\n' },
      format: 'json',
    });

    const doc = JSON.parse(result.text) as {
      version: number;
      files: { path: string; diagnostics: { id: string }[] }[];
      summary: { error: number };
    };
    expect(doc.version).toBe(1);
    expect(doc.files[0]?.path).toBe('Mods/A/mod.ini');
    expect(doc.files[0]?.diagnostics.map((d) => d.id)).toContain('XM201');
    expect(doc.summary.error).toBeGreaterThan(0);
  });

  it('resolves library references against the indexed package and includes between files', async () => {
    const result = await callTool(await client(), 'lint_ini', {
      files: {
        'Mods/A/mod.ini':
          '[Include Other]\ninclude = other.ini\n\n[TextureOverrideA]\nhash = 11111111\nrun = CommandList\\Demo\\SetThings\nrun = CommandList\\Shared\\Thing\n',
        'Mods/A/other.ini': 'namespace = Shared\n\n[CommandListThing]\n',
      },
    });

    expect(result.isError).toBe(false);
    expect(result.text).toContain('0 errors');
  });

  it('never reads the server file system for includes or paths', async () => {
    const result = await callTool(await client(), 'lint_ini', {
      files: { 'a.ini': '[Include Evil]\ninclude = ..\\..\\..\\..\\etc\\hosts\n' },
      format: 'json',
    });

    expect(result.text).toContain('XM207');
  });

  it('rejects bad paths, non-ini files and duplicates', async () => {
    const c = await client();
    const bad = ['../x.ini', '/abs.ini', 'C:\\x.ini', 'a/../b.ini', 'x.txt', ''];

    for (const path of bad) {
      expect((await callTool(c, 'lint_ini', { files: { [path]: '' } })).isError, path).toBe(true);
    }
    const dup = await callTool(c, 'lint_ini', { files: { 'A.ini': '', 'a.INI': '' } });
    expect(dup.text).toContain('duplicate');
  });
});

describe('lint_ini caps', () => {
  it('refuses more bytes than MAX_LINT_BYTES and more files than MAX_LINT_FILES', async () => {
    const { app } = await indexedApp({ maxLintBytes: 100, maxLintFiles: 2 });
    apps.push(app);
    const c = await connectClient(app);

    const big = await callTool(c, 'lint_ini', { files: { 'a.ini': 'x'.repeat(101) } });
    expect(big.isError).toBe(true);
    expect(big.text).toContain('input too large');
    const many = await callTool(c, 'lint_ini', {
      files: { 'a.ini': '', 'b.ini': '', 'c.ini': '' },
    });
    expect(many.text).toContain('too many files');
    expect((await callTool(c, 'lint_ini', { files: { 'a.ini': 'x'.repeat(100) } })).isError).toBe(
      false,
    );
  });

  it('does not log the submitted text', async () => {
    const logs: string[] = [];
    const { app } = await indexedApp();
    apps.push(app);
    app.tools.logger = {
      info: (m, f) => logs.push(JSON.stringify({ m, f })),
      warn() {},
      error() {},
    };
    const c = await connectClient(app);

    await callTool(c, 'lint_ini', { files: { 'a.ini': '; SECRET-TEXT\n[CommandListA]\n' } });

    expect(logs.join('\n')).toContain('lint_ini');
    expect(logs.join('\n')).not.toContain('SECRET');
  });
});

describe('resources', () => {
  it('serves the spec documents', async () => {
    const c = await client();

    const list = await c.listResources();
    expect(list.resources.map((r) => r.uri)).toContain('xxmi://spec/latest/commands.json');
    const read = await c.readResource({ uri: 'xxmi://spec/latest/sections.json' });
    const doc = JSON.parse((read.contents[0] as { text: string }).text) as { sections: unknown[] };
    expect(doc.sections.length).toBeGreaterThan(10);
  });
});
