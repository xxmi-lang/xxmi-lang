/**
 * Bundles the release files: one zip per platform (`xxmi[.exe]` and `xxmi-lsp[.exe]`, the same
 * binary under both names, LICENSE, README.txt), the renamed VS Code extension, and SHA256SUMS over
 * all of them.
 *
 *   node scripts/bundle-release.ts --version 0.1.1 [--bin dist/bin] [--vsix path] [--out dist/release]
 *   node scripts/bundle-release.ts --version 0.1.1 --host      # only this machine, from dist/bin/xxmi[.exe]
 *
 * Reads the loose `xxmi-<os>-<arch>[.exe]` files that `pnpm build:binaries --target <os>-<arch>` writes,
 * one per target (release.yml downloads them into dist/bin).
 * Needs the `zip` command (preinstalled on Linux and macOS runners). The binaries are chmod +x
 * before zipping and `zip -X` keeps the Unix mode, so they stay executable after `unzip`.
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

const REPO = resolve(import.meta.dirname, '..');
const { values } = parseArgs({
  options: {
    version: { type: 'string' },
    bin: { type: 'string' },
    vsix: { type: 'string' },
    out: { type: 'string' },
    host: { type: 'boolean' },
  },
});
if (!values.version) throw new Error('--version is required');
const version = values.version;
const binDir = resolve(values.bin ?? join(REPO, 'dist/bin'));
const out = resolve(values.out ?? join(REPO, 'dist/release'));

// Also the target name build-binaries.ts writes the binary under.
const PLATFORMS = [
  'windows-x64',
  'windows-arm64',
  'linux-x64',
  'linux-arm64',
  'macos-x64',
  'macos-arm64',
];

const README = `xxmi-lang ${version}

Two programs, both single files with nothing else to install:

  xxmi-lsp   the language server for 3DMigoto / XXMI mod .ini files (Kate, Claude Code, Neovim, ...)
  xxmi       the command-line linter: xxmi lint path/to/mod   (also "xxmi lsp --stdio")

Put both on your PATH. Step-by-step instructions for every system:
https://github.com/xxmi-lang/xxmi-lang/blob/main/INSTALL.md

Licensed GPL-3.0-or-later, see LICENSE.
`;

mkdirSync(out, { recursive: true });
const files: string[] = [];
const tmp = mkdtempSync(join(tmpdir(), 'xxmi-bundle-'));

function bundle(zipName: string, binary: string, exe: string): void {
  const stage = join(tmp, zipName);
  mkdirSync(stage);
  if (!existsSync(binary)) throw new Error(`missing ${binary}; run pnpm build:binaries`);
  for (const name of ['xxmi', 'xxmi-lsp']) {
    const dest = join(stage, `${name}${exe}`);
    copyFileSync(binary, dest);
    chmodSync(dest, 0o755);
  }
  copyFileSync(join(REPO, 'LICENSE'), join(stage, 'LICENSE'));
  writeFileSync(join(stage, 'README.txt'), README);
  const zip = join(out, `xxmi-${version}-${zipName}.zip`);
  rmSync(zip, { force: true });
  execFileSync('zip', ['-X', '-q', zip, `xxmi${exe}`, `xxmi-lsp${exe}`, 'LICENSE', 'README.txt'], {
    cwd: stage,
  });
  files.push(zip);
  console.log(`bundled ${zip}`);
}

try {
  if (values.host) {
    const os = ({ win32: 'windows', darwin: 'macos', linux: 'linux' } as Record<string, string>)[
      process.platform
    ];
    if (!os) throw new Error(`unsupported platform ${process.platform}`);
    const exe = process.platform === 'win32' ? '.exe' : '';
    bundle(`${os}-${process.arch}`, join(binDir, `xxmi${exe}`), exe);
  } else {
    for (const zipName of PLATFORMS) {
      const exe = zipName.startsWith('windows') ? '.exe' : '';
      bundle(zipName, join(binDir, `xxmi-${zipName}${exe}`), exe);
    }
  }
  if (values.vsix) {
    const dest = join(out, `xxmi-lang-${version}.vsix`);
    copyFileSync(resolve(values.vsix), dest);
    files.push(dest);
  }
  const sums = files.map((f) => {
    const hash = createHash('sha256').update(readFileSync(f)).digest('hex');
    return `${hash}  ${f.slice(out.length + 1)}\n`;
  });
  writeFileSync(join(out, 'SHA256SUMS'), sums.join(''));
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
