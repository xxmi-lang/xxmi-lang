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
 * Each zip also carries `editors/` (the files the manifest ships, plus the versioned manifest,
 * and the hand-merged snippet folders).
 * Needs the `zip` command (preinstalled on Linux and macOS runners). The binaries are chmod +x
 * before zipping and `zip -X` keeps the Unix mode, so they stay executable after `unzip`.
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import {
  MANIFEST_PATH,
  loadManifest,
  shippedSources,
  versionedManifest,
} from './editors-manifest.ts';
import {
  OS_BY_NODE_PLATFORM,
  RELEASE_TARGETS,
  exeSuffix,
  releaseBinaryName,
  targetName,
} from './platforms.ts';

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

// Snippets the user merges into their own config by hand, so the manifest doesn't list them.
const MANUAL_EDITOR_DIRS = ['editors/helix', 'editors/sublime'];

const README = `xxmi-lang ${version}

Two programs, both single files with nothing else to install:

  xxmi-lsp   the language server for 3DMigoto / XXMI mod .ini files (Kate, Claude Code, Neovim, ...)
  xxmi       the command-line linter: xxmi lint path/to/mod   (also "xxmi lsp --stdio")

Put both on your PATH. Step-by-step instructions for every system:
https://github.com/xxmi-lang/xxmi-lang/blob/main/INSTALL.md

editors/ holds editor config; editors/manifest.json lists where each file goes
(see https://github.com/xxmi-lang/xxmi-lang/blob/main/editors/README.md).

Licensed GPL-3.0-or-later, see LICENSE.
`;

mkdirSync(out, { recursive: true });
const files: string[] = [];
const tmp = mkdtempSync(join(tmpdir(), 'xxmi-bundle-'));

function stageEditors(stage: string): string[] {
  const manifest = loadManifest(REPO);
  const sources = shippedSources(manifest);

  for (const source of sources) {
    const dest = join(stage, source);
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(join(REPO, source), dest);
  }

  for (const dir of MANUAL_EDITOR_DIRS)
    cpSync(join(REPO, dir), join(stage, dir), { recursive: true });

  writeFileSync(join(stage, MANIFEST_PATH), versionedManifest(manifest, version));

  return [...sources, ...MANUAL_EDITOR_DIRS, MANIFEST_PATH];
}

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
  const editorFiles = stageEditors(stage);
  const zip = join(out, `xxmi-${version}-${zipName}.zip`);
  rmSync(zip, { force: true });
  execFileSync(
    'zip',
    [
      '-X',
      '-q',
      '-r',
      zip,
      `xxmi${exe}`,
      `xxmi-lsp${exe}`,
      'LICENSE',
      'README.txt',
      ...editorFiles,
    ],
    {
      cwd: stage,
    },
  );
  files.push(zip);
  console.log(`bundled ${zip}`);
}

try {
  if (values.host) {
    const os = OS_BY_NODE_PLATFORM[process.platform];
    if (!os) throw new Error(`unsupported platform ${process.platform}`);
    const exe = exeSuffix(os);
    bundle(`${os}-${process.arch}`, join(binDir, `xxmi${exe}`), exe);
  } else {
    for (const t of RELEASE_TARGETS) {
      bundle(targetName(t), join(binDir, releaseBinaryName(t)), exeSuffix(t.os));
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
