/**
 * Packs the .NET library (docs/10-dotnet.md): XxmiLang.Core (the definitions), one XxmiLang.Native.<rid>
 * per platform (each carries the single-file `xxmi` for that platform), XxmiLang (the implementation) and
 * the XxmiLang.All meta package. Packed in that order, which is also the nuget.org push order (All last).
 *
 *   pnpm build:binaries                      # first: dist/bin/xxmi-<os>-<arch>[.exe]
 *   node scripts/pack-dotnet.ts [--bin dist/bin] [--out dist/nupkg] [--version X.Y.Z]
 *
 * `--host` packs XxmiLang.Core, XxmiLang and only this machine's runtime package, from dist/bin/xxmi (what
 * `pnpm build:binaries --target host` writes), for trying the packages locally.
 */
import { execFileSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

/** .NET runtime id -> suffix of the binary `pnpm build:binaries` writes. */
export const RUNTIMES: Record<string, string> = {
  'win-x64': 'windows-x64.exe',
  'win-arm64': 'windows-arm64.exe',
  'linux-x64': 'linux-x64',
  'linux-arm64': 'linux-arm64',
  'osx-x64': 'darwin-x64',
  'osx-arm64': 'darwin-arm64',
};

const REPO = resolve(import.meta.dirname, '..');
const DOTNET = join(REPO, 'dotnet');

function dotnetRid(): string {
  const oses: Partial<Record<NodeJS.Platform, string>> = {
    win32: 'win',
    linux: 'linux',
    darwin: 'osx',
  };
  const arches: Partial<Record<NodeJS.Architecture, string>> = { x64: 'x64', arm64: 'arm64' };
  const os = oses[process.platform];
  const arch = arches[process.arch];
  if (!os || !arch) throw new Error(`no xxmi build for ${process.platform} ${process.arch}`);
  return `${os}-${arch}`;
}

function dotnet(...args: string[]): void {
  console.log(`dotnet ${args.join(' ')}`);
  execFileSync('dotnet', args, { stdio: 'inherit', cwd: DOTNET });
}

const { values } = parseArgs({
  options: {
    bin: { type: 'string' },
    out: { type: 'string' },
    version: { type: 'string' },
    host: { type: 'boolean', default: false },
  },
});
const bin = resolve(values.bin ?? join(REPO, 'dist/bin'));
const out = resolve(values.out ?? join(REPO, 'dist/nupkg'));
const version = values.version ? [`-p:Version=${values.version}`] : [];
const rids = values.host ? [dotnetRid()] : Object.keys(RUNTIMES);

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const staging = mkdtempSync(join(tmpdir(), 'xxmi-pack-'));
try {
  dotnet('pack', 'src/XxmiLang.Core', '-c', 'Release', '-o', out, ...version);
  for (const rid of rids) {
    const exe = rid.startsWith('win-') ? 'xxmi.exe' : 'xxmi';
    const suffix = RUNTIMES[rid] as string;
    const source = values.host ? join(bin, exe) : join(bin, `xxmi-${suffix}`);
    if (!existsSync(source))
      throw new Error(`${source} is missing; run \`pnpm build:binaries\` first`);
    // The package holds the file as `xxmi[.exe]`, whatever the release calls it.
    const dir = join(staging, rid);
    mkdirSync(dir);
    copyFileSync(source, join(dir, exe));
    dotnet(
      'pack',
      'src/XxmiLang.Native',
      '-c',
      'Release',
      '-o',
      out,
      `-p:NativeRid=${rid}`,
      `-p:XxmiBinary=${join(dir, exe)}`,
      ...version,
    );
  }
  dotnet('pack', 'src/XxmiLang', '-c', 'Release', '-o', out, ...version);
  // The meta package needs all six runtime packages to be installable.
  if (!values.host) dotnet('pack', 'src/XxmiLang.All', '-c', 'Release', '-o', out, ...version);
} finally {
  rmSync(staging, { recursive: true, force: true });
}

for (const name of readdirSync(out).sort()) {
  const mb = statSync(join(out, name)).size / 1024 / 1024;
  console.log(`${mb.toFixed(1).padStart(7)} MB  ${name}`);
}
