import { describe, expect, it } from 'vitest';
import { bundledBinaryPath } from '../src/platform.ts';

describe('bundledBinaryPath', () => {
  it.each([
    ['win32', 'x64', 'bin/windows-x64/xxmi.exe'],
    ['win32', 'arm64', 'bin/windows-arm64/xxmi.exe'],
    ['linux', 'x64', 'bin/linux-x64/xxmi'],
    ['linux', 'arm64', 'bin/linux-arm64/xxmi'],
    ['darwin', 'x64', 'bin/macos-x64/xxmi'],
    ['darwin', 'arm64', 'bin/macos-arm64/xxmi'],
  ])('%s %s', (platform, arch, expected) => {
    expect(bundledBinaryPath(platform, arch)).toBe(expected);
  });

  it('is undefined for unsupported platforms and architectures', () => {
    expect(bundledBinaryPath('freebsd', 'x64')).toBeUndefined();
    expect(bundledBinaryPath('linux', 'ia32')).toBeUndefined();
  });
});
