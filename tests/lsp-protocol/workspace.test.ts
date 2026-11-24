/**
 * A broken xxmi.toml and files that appear or vanish after startup, in a live server
 * (docs/03-lsp.md "Robustness"): each test starts its own server on a throwaway folder.
 */
import { afterEach, describe, expect, it } from 'vitest';
import type { Diagnostic } from 'vscode-languageserver-protocol/node';
import { Session } from './session.ts';

const codes = (d: Diagnostic[]): string[] => d.map((x) => String(x.code));
const exactly =
  (...expected: string[]) =>
  (d: Diagnostic[]): boolean =>
    codes(d).join() === expected.join();

const LIB_REF = 'run = CommandList\\Lib\\Foo';
const LIB_TEXT = 'namespace = Lib\n[CommandListFoo]\n';
const MOD_HEAD = '[TextureOverrideA]\nhash = 1a2b3c4d\n';
const MOD_TEXT = `${MOD_HEAD}${LIB_REF}\n`;
const BROKEN_TEXT = `${MOD_HEAD}run = CommandListNope\n`;

let session: Session | undefined;
const begin = (): Session => (session = new Session());
afterEach(() => {
  session?.stop();
  session = undefined;
});

describe('xxmi-lsp with a broken xxmi.toml', { timeout: 30_000 }, () => {
  it.each([
    ['unparsable toml', 'profile = [\n'],
    ['unknown profile', 'profile = "nope"\n'],
    ['invalid rule value', '[rules]\nXM201 = "loud"\n'],
  ])('lints with defaults and reports once: %s', async (_name, toml) => {
    const s = begin();
    s.write('xxmi.toml', toml);
    const path = s.write('a.ini', BROKEN_TEXT);
    const first = s.next(path, exactly('XM201'));
    await s.start();
    await s.open(path, BROKEN_TEXT);
    await first;

    const edited = s.next(path, exactly('XM201'));
    await s.replace(path, `${BROKEN_TEXT}; edited\n`);
    await edited;

    expect(s.client.logs.filter((m) => m.includes('xxmi.toml'))).toHaveLength(1);
  });
});

describe('xxmi-lsp with files created after startup', { timeout: 30_000 }, () => {
  it('loads a created file the index would have loaded, in one refresh', async () => {
    const s = begin();
    const path = s.write('a.ini', MOD_TEXT);
    const initial = s.next(path, exactly('XM206'));
    await s.start();
    await s.open(path, MOD_TEXT);
    await initial;

    const lib = s.write('Lib/lib.ini', LIB_TEXT);
    const more = Array.from({ length: 50 }, (_, i) => s.write(`more/m${i}.ini`, '[Constants]\n'));
    const hidden = s.write('DISABLEDLib/lib.ini', LIB_TEXT.replace('Lib', 'Other'));
    const notIni = s.write('Lib/notes.txt', 'x');
    const refreshed = s.next(path, exactly());
    await s.created(lib, ...more, hidden, notIni);

    await refreshed;
  });

  it('keeps a namespace of a created file in a skipped folder unresolved', async () => {
    const s = begin();
    const path = s.write('a.ini', `${MOD_HEAD}run = CommandList\\Other\\Foo\n`);
    const initial = s.next(path, exactly('XM206'));
    await s.start();
    await s.open(path, `${MOD_HEAD}run = CommandList\\Other\\Foo\n`);
    await initial;

    const lib = s.write('DISABLEDLib/lib.ini', 'namespace = Other\n[CommandListFoo]\n');
    const sentinel = s.write('Lib/lib.ini', LIB_TEXT);
    const probe = s.next(path, () => true);
    await s.created(lib, sentinel);

    expect(codes(await probe)).toEqual(['XM206']);
  });

  it('unloads a deleted file', async () => {
    const s = begin();
    const lib = s.write('Lib/lib.ini', LIB_TEXT);
    const path = s.write('a.ini', MOD_TEXT);
    const initial = s.next(path, exactly());
    await s.start();
    await s.open(path, MOD_TEXT);
    await initial;

    const after = s.next(path, exactly('XM206'));
    await s.deleted(lib);

    await after;
  });
});
