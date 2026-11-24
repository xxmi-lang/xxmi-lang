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
