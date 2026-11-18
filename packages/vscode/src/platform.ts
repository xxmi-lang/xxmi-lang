/** Maps a Node platform/arch pair to the bundled server binary inside the extension. */
export const BUNDLED_OS = ['windows', 'linux', 'macos'] as const;
export const BUNDLED_ARCH = ['x64', 'arm64'] as const;

const OS_BY_PLATFORM: Record<string, (typeof BUNDLED_OS)[number]> = {
  win32: 'windows',
  linux: 'linux',
  darwin: 'macos',
};

export function binaryName(os: string): string {
  return os === 'windows' ? 'xxmi.exe' : 'xxmi';
}

/** Path relative to the extension root, or undefined when no binary is bundled for the pair. */
export function bundledBinaryPath(platform: string, arch: string): string | undefined {
  const os = OS_BY_PLATFORM[platform];
  if (!os || !(BUNDLED_ARCH as readonly string[]).includes(arch)) return undefined;
  return `bin/${os}-${arch}/${binaryName(os)}`;
}
