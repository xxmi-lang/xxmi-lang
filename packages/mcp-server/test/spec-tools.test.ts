import { afterEach, describe, expect, it } from 'vitest';
import type { App } from '../src/app.ts';
import { callTool, connectClient, indexedApp, SHA_A, SHA_B, TAG_SHA } from './helpers.ts';
import { NEWER_KEY, versionedFixtures } from './spec-fixtures.ts';

const apps: App[] = [];
afterEach(async () => {
  await Promise.all(apps.splice(0).map((a) => a.close()));
});

async function client(options: { dropBaseline?: boolean } = {}) {
  const { remotes, dirs } = versionedFixtures();
  if (options.dropBaseline) delete remotes['migoto'];
  const { app } = await indexedApp({}, remotes, { dirs });
  apps.push(app);
  return connectClient(app);
}

const lookupKey = (c: Awaited<ReturnType<typeof client>>, dll_ref?: string) =>
  callTool(c, 'spec_lookup', {
    name: NEWER_KEY,
    kind: 'key',
    section: 'Resource',
    ...(dll_ref ? { dll_ref } : {}),
  });

const INI = `[ResourceX]\n${NEWER_KEY} = 1\n`;

describe('spec_lookup with dll_ref', () => {
  it('answers differently for two versions whose sources differ in one key', async () => {
    const c = await client();

    const release = await lookupKey(c, 'latest-release');
    const prerelease = await lookupKey(c, 'latest-prerelease');
    const head = await lookupKey(c, 'head');

    expect(release.text).toContain('No spec entry named "newer_only"');
    expect(prerelease.text).toContain(`key ${NEWER_KEY}`);
    expect(head.text).toContain(`key ${NEWER_KEY}`);
  });

  it('uses the latest release when no dll_ref is given', async () => {
    const text = (await lookupKey(await client())).text;

    expect(text).toContain('No spec entry');
    expect(text).toMatch(/\nspec: XXMI-Libs-Package v1\.2\.3 \(release, bbbbbbb\)$/);
  });

  it('ends with the spec line and carries the spec block in the structured result', async () => {
    const c = await client();

    const text = (await lookupKey(c, 'v1.3.1')).text;
    const json = JSON.parse(
      (await callTool(c, 'spec_lookup', { name: 'store', dll_ref: 'v1.3.1', format: 'json' })).text,
    ) as { spec: Record<string, unknown> };

    expect(text.split('\n').at(-1)).toBe('spec: XXMI-Libs-Package v1.3.1 (prerelease, ccccccc)');
    expect(json.spec).toMatchObject({
      source: 'extracted',
      dllRef: 'v1.3.1',
      dllSha: TAG_SHA,
      tag: 'v1.3.1',
      kind: 'prerelease',
      dllCommitDate: '2026-01-01T00:00:00Z',
    });
    expect(json.spec).not.toHaveProperty('note');
  });

  it('serves the bundled spec for "bundled" and fails on an unknown ref', async () => {
    const c = await client();

    const bundled = await callTool(c, 'spec_lookup', { name: 'store', dll_ref: 'bundled' });
    const unknown = await callTool(c, 'spec_lookup', { name: 'store', dll_ref: 'v9.9.9' });

    expect(bundled.text).toMatch(/\nspec: bundled \(\w{7}\)$/);
    expect(unknown.isError).toBe(true);
    expect(unknown.text).toContain('Indexed:');
  });

  it('answers from the bundled spec with a note while the version is still building', async () => {
    const c = await client({ dropBaseline: true });

    const { text } = await callTool(c, 'spec_lookup', { name: 'store', dll_ref: 'v1.3.1' });
    const json = JSON.parse(
      (await callTool(c, 'spec_lookup', { name: 'store', dll_ref: 'v1.3.1', format: 'json' })).text,
    ) as { spec: { source: string; note: string } };

    expect(text).toMatch(
      /\nspec: bundled \(\w{7}\); requested v1\.3\.1 \(ccccccc\) is still building$/,
    );
    expect(json.spec.source).toBe('bundled');
    expect(json.spec.note).toContain('Requested v1.3.1');
  });
});

describe('spec_list with dll_ref', () => {
  it('lists the keys of the requested version', async () => {
    const c = await client();

    const old = await callTool(c, 'spec_list', {
      kind: 'key',
      section: 'Resource',
      dll_ref: 'v1.2.3',
    });
    const head = await callTool(c, 'spec_list', {
      kind: 'key',
      section: 'Resource',
      dll_ref: 'head',
    });

    expect(old.text).not.toContain(NEWER_KEY);
    expect(head.text).toContain(NEWER_KEY);
    expect(head.text.split('\n').at(-1)).toBe('spec: XXMI-Libs-Package master (head, aaaaaaa)');
  });
});

describe('lint_ini with dll_ref', () => {
  const lint = (c: Awaited<ReturnType<typeof client>>, dll_ref?: string) =>
    callTool(c, 'lint_ini', {
      files: { 'Mods/M/mod.ini': INI },
      format: 'json',
      ...(dll_ref ? { dll_ref } : {}),
    });

  const ids = (text: string): string[] =>
    (JSON.parse(text) as { files: { diagnostics: { id: string }[] }[] }).files.flatMap((f) =>
      f.diagnostics.map((d) => d.id),
    );

  it('flags a key the older DLL does not read and accepts it on the newer one', async () => {
    const c = await client();

    expect(ids((await lint(c, 'v1.2.3')).text)).toContain('XM102');
    expect(ids((await lint(c, 'head')).text)).not.toContain('XM102');
    expect(ids((await lint(c)).text)).toContain('XM102');
  });

  it('reports the spec block in the result and the line in the text', async () => {
    const c = await client();

    const json = JSON.parse((await lint(c, 'head')).text) as { spec: { dllSha: string } };
    const text = (
      await callTool(c, 'lint_ini', { files: { 'Mods/M/mod.ini': INI }, dll_ref: 'head' })
    ).text;

    expect(json.spec.dllSha).toBe(SHA_A);
    expect(text.split('\n').at(-1)).toBe('spec: XXMI-Libs-Package master (head, aaaaaaa)');
  });
});

describe('find_implementation', () => {
  it('uses the spec of the DLL version it searches', async () => {
    const c = await client();

    const text = (await callTool(c, 'find_implementation', { feature: 'store', ref: 'v1.2.3' }))
      .text;

    expect(text.split('\n').at(-1)).toBe('spec: XXMI-Libs-Package v1.2.3 (release, bbbbbbb)');
  });
});

describe('list_versions', () => {
  interface Listed {
    defaultSpec: Record<string, unknown>;
    spec: { dllCommit: string };
    repos: { alias: string; default: Record<string, unknown>; tags: Record<string, unknown>[] }[];
  }
  const list = async (c: Awaited<ReturnType<typeof client>>) =>
    JSON.parse((await callTool(c, 'list_versions', { format: 'json' })).text) as Listed;

  it('shows the spec status of every DLL version and the default spec', async () => {
    const c = await client();

    const json = await list(c);
    const dll = json.repos.find((r) => r.alias === 'dll');

    expect(dll?.default).toMatchObject({ name: 'master', spec: 'ok' });
    expect(dll?.tags.map((t) => [t['name'], t['spec']])).toEqual([
      ['v1.3.1', 'ok'],
      ['v1.2.3', 'ok'],
    ]);
    expect(json.repos.find((r) => r.alias === 'zzmi')?.default).not.toHaveProperty('spec');
    expect(json.defaultSpec).toEqual({
      source: 'extracted',
      dllSha: SHA_B,
      tag: 'v1.2.3',
      kind: 'release',
    });
    expect(json.spec.dllCommit).toMatch(/^[0-9a-f]{40}$/);
  });

  it('shows pending specs and the bundled default while they are building', async () => {
    const c = await client({ dropBaseline: true });

    const json = await list(c);
    const text = (await callTool(c, 'list_versions', {})).text;

    expect(json.repos.find((r) => r.alias === 'dll')?.default).toMatchObject({ spec: 'pending' });
    expect(json.defaultSpec).toMatchObject({ source: 'bundled', tag: null, kind: null });
    expect(text).toContain('default spec: bundled');
    expect(text).toContain('specs: master pending');
  });
});

describe('spec resources', () => {
  it('serves the default spec, a version by ref, and a file of a version', async () => {
    const c = await client();
    const read = async (uri: string) =>
      JSON.parse(((await c.readResource({ uri })).contents[0] as { text: string }).text) as {
        sections: { name: string; keys: { name: string }[] }[];
        commands?: unknown[];
        _spec: { dllSha: string };
      };
    const hasKey = (doc: Awaited<ReturnType<typeof read>>): boolean =>
      doc.sections.some((s) => s.keys.some((k) => k.name === NEWER_KEY));

    const byDefault = await read('xxmi://spec');
    const head = await read('xxmi://spec/head');
    const file = await read('xxmi://spec/head/sections.json');

    expect(byDefault._spec.dllSha).toBe(SHA_B);
    expect(hasKey(byDefault)).toBe(false);
    expect(head._spec.dllSha).toBe(SHA_A);
    expect(hasKey(head)).toBe(true);
    expect(head.commands?.length).toBeGreaterThan(0);
    expect(file.commands).toBeUndefined();
    expect(hasKey(file)).toBe(true);
  });

  it('lists the spec files of the default version', async () => {
    const list = await (await client()).listResources();

    expect(list.resources.map((r) => r.uri)).toContain('xxmi://spec/latest-release/commands.json');
  });
});
