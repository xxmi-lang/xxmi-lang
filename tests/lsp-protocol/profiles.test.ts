/**
 * Game profile selection in a live server (docs/01-language-model.md §5): each test starts its
 * own server on a throwaway folder, since the profile is a property of the whole workspace.
 */
import { afterEach, describe, expect, it } from 'vitest';
import type { Diagnostic } from 'vscode-languageserver-protocol/node';
import { Session } from './session.ts';

const ZZMI_REF = 'run = CommandList\\ZZMI\\SetTextures';
const WWMI_REF = 'run = CommandList\\WWMIv1\\BackupRestoreVertexTextureSlots';

const hasCode =
  (code: string) =>
  (d: Diagnostic[]): boolean =>
    d.some((x) => x.code === code);
// XM201 is an unresolved reference, XM206 one whose library namespace is not loaded.
const unresolved = (d: Diagnostic[]): boolean => hasCode('XM201')(d) || hasCode('XM206')(d);
const resolved = (d: Diagnostic[]): boolean => !unresolved(d);

let session: Session | undefined;
const begin = (): Session => {
  session = new Session();
  return session;
};
afterEach(() => {
  session?.stop();
  session = undefined;
});

describe('xxmi-lsp game profile', { timeout: 30_000 }, () => {
  it('honors the profile of xxmi.toml over detection', async () => {
    const s = begin();
    s.write('xxmi.toml', 'profile = "wwmi"\n');
    const path = s.write('a.ini', `[CommandListA]\n${ZZMI_REF}\n`);
    const wait = s.next(path, hasCode('XM206'));
    await s.start();
    await s.open(path, `[CommandListA]\n${ZZMI_REF}\n`);

    expect(hasCode('XM206')(await wait)).toBe(true);
  });

  it('re-detects when the open document gains its first library reference', async () => {
    const s = begin();
    const path = s.write('a.ini', '[CommandListA]\nrun = CommandListNope\n');
    const initial = s.next(path, hasCode('XM201'));
    await s.start();
    await s.open(path, '[CommandListA]\nrun = CommandListNope\n');
    await initial;

    const after = s.next(path, resolved);
    await s.replace(path, `[CommandListA]\n${ZZMI_REF}\n`);

    expect(unresolved(await after)).toBe(false);
  });

  it('follows xxmi.miPackage as the setting changes', async () => {
    const s = begin();
    const path = s.write('a.ini', `[CommandListA]\n${ZZMI_REF}\n`);
    await s.start();
    await s.open(path, `[CommandListA]\n${ZZMI_REF}\n`);

    const forced = s.next(path, hasCode('XM206'));
    await s.changeSettings({ miPackage: 'wwmi' });
    expect(hasCode('XM206')(await forced)).toBe(true);

    const detected = s.next(path, resolved);
    await s.changeSettings({ miPackage: 'auto' });
    expect(unresolved(await detected)).toBe(false);
  });

  it('re-detects when a file with another library is opened, and when it is deleted', async () => {
    const s = begin();
    const first = s.write('a.ini', `[CommandListA]\n${WWMI_REF}\n`);
    const initial = s.next(first, resolved);
    await s.start();
    await s.open(first, `[CommandListA]\n${WWMI_REF}\n`);
    await initial;

    const second = s.write('b.ini', `[CommandListB]\n${ZZMI_REF}\n`);
    const ambiguous = s.next(first, hasCode('XM206'));
    await s.open(second, `[CommandListB]\n${ZZMI_REF}\n`);
    expect(hasCode('XM206')(await ambiguous)).toBe(true);

    const restored = s.next(first, resolved);
    await s.close(second);
    await s.deleted(second);
    expect(unresolved(await restored)).toBe(false);
  });
});
