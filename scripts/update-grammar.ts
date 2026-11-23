/**
 * Copies the generated parser sources of our tree-sitter-migoto fork into
 * rust/crates/tree-sitter-migoto/vendor and records where they came from in SOURCE.md.
 *
 *   XXMI_GRAMMAR_SRC=../tree-sitter-migoto pnpm grammar:update [--allow-dirty]
 *
 * The fork checks in src/parser.c, so no tree-sitter CLI is needed here.
 */
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const CRATE_DIR = resolve(import.meta.dirname, '../rust/crates/tree-sitter-migoto');
const VENDOR_DIR = join(CRATE_DIR, 'vendor');
const UPSTREAM = 'https://github.com/lupomikti/tree-sitter-migoto';

const args = process.argv.slice(2);
const allowDirty = args.includes('--allow-dirty');
const src = args.find((a) => !a.startsWith('--')) ?? process.env.XXMI_GRAMMAR_SRC;
if (!src) {
  console.error('usage: XXMI_GRAMMAR_SRC=<fork checkout> pnpm grammar:update [--allow-dirty]');
  process.exit(2);
}
const forkDir = resolve(src);

function git(gitArgs: string[]): string {
  return execFileSync('git', gitArgs, {
    cwd: forkDir,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
}

function gitOrNull(gitArgs: string[]): string | null {
  try {
    return git(gitArgs) || null;
  } catch {
    return null;
  }
}

function copyParserSources(): void {
  rmSync(VENDOR_DIR, { recursive: true, force: true });
  mkdirSync(VENDOR_DIR, { recursive: true });
  for (const file of ['parser.c', 'scanner.c']) {
    cpSync(join(forkDir, 'src', file), join(VENDOR_DIR, file));
  }
  cpSync(join(forkDir, 'src/tree_sitter'), join(VENDOR_DIR, 'tree_sitter'), { recursive: true });
}

function writeSourceNote(): void {
  const repository = gitOrNull(['remote', 'get-url', 'origin']) ?? 'unknown';
  const branch = gitOrNull(['rev-parse', '--abbrev-ref', 'HEAD']) ?? 'unknown';
  const commit = gitOrNull(['rev-parse', '--short=7', 'HEAD']) ?? 'unknown';
  const license = readFileSync(join(forkDir, 'LICENSE'), 'utf8').trimEnd();

  writeFileSync(
    join(CRATE_DIR, 'SOURCE.md'),
    `# Vendored grammar sources

- Repository: ${repository} (fork of ${UPSTREAM})
- Branch \`${branch}\`, commit \`${commit}\`
- Files: \`src/parser.c\`, \`src/scanner.c\` (external scanner) and \`src/tree_sitter/*.h\`, copied unmodified into \`vendor/\`
- Generated with tree-sitter CLI as checked in upstream; regenerate there, then run \`pnpm grammar:update\`

\`\`\`
${license}
\`\`\`
`,
  );
}

if (!allowDirty && git(['status', '--porcelain']) !== '') {
  console.error(`${forkDir} has uncommitted changes; commit them or pass --allow-dirty.`);
  process.exit(1);
}

copyParserSources();
writeSourceNote();
console.log(`Vendored ${forkDir} into ${VENDOR_DIR}`);
