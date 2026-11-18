/** The release targets, shared by build-binaries.ts, bundle-release.ts and the VS Code staging. */
export type Os = 'windows' | 'linux' | 'macos';
export type Arch = 'x64' | 'arm64';

export interface ReleaseTarget {
  os: Os;
  arch: Arch;
  /** Rust target triple. */
  triple: string;
}

export const RELEASE_TARGETS: readonly ReleaseTarget[] = [
  { os: 'windows', arch: 'x64', triple: 'x86_64-pc-windows-msvc' },
  { os: 'windows', arch: 'arm64', triple: 'aarch64-pc-windows-msvc' },
  { os: 'linux', arch: 'x64', triple: 'x86_64-unknown-linux-musl' },
  { os: 'linux', arch: 'arm64', triple: 'aarch64-unknown-linux-musl' },
  { os: 'macos', arch: 'x64', triple: 'x86_64-apple-darwin' },
  { os: 'macos', arch: 'arm64', triple: 'aarch64-apple-darwin' },
];

export const OS_BY_NODE_PLATFORM: Record<string, Os> = {
  win32: 'windows',
  linux: 'linux',
  darwin: 'macos',
};

/** The release name of a target, e.g. macos-arm64. */
export function targetName(t: Pick<ReleaseTarget, 'os' | 'arch'>): string {
  return `${t.os}-${t.arch}`;
}

export function exeSuffix(os: string): string {
  return os === 'windows' ? '.exe' : '';
}

/** The loose file build:binaries writes and the release reads, e.g. xxmi-windows-x64.exe. */
export function releaseBinaryName(t: Pick<ReleaseTarget, 'os' | 'arch'>): string {
  return `xxmi-${targetName(t)}${exeSuffix(t.os)}`;
}
