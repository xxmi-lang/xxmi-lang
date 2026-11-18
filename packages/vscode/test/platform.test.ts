import { describe, expect, it } from 'vitest';
import {
  OS_BY_NODE_PLATFORM,
  RELEASE_TARGETS,
  releaseBinaryName,
  targetName,
} from '../../../scripts/platforms.ts';
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

  // The runtime code is bundled into the VSIX, so it keeps its own copy of the shared list.
  it('agrees with scripts/platforms.ts', () => {
    for (const [platform, os] of Object.entries(OS_BY_NODE_PLATFORM)) {
      for (const t of RELEASE_TARGETS.filter((r) => r.os === os)) {
        const path = bundledBinaryPath(platform, t.arch);
        expect(path).toBe(
          `bin/${targetName(t)}/${releaseBinaryName(t).replace(`-${targetName(t)}`, '')}`,
        );
      }
    }
    expect(RELEASE_TARGETS).toHaveLength(6);
  });

  it('is undefined for unsupported platforms and architectures', () => {
    expect(bundledBinaryPath('freebsd', 'x64')).toBeUndefined();
    expect(bundledBinaryPath('linux', 'ia32')).toBeUndefined();
  });
});
