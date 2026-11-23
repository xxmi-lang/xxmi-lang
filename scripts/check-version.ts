// The release version lives in rust/Cargo.toml ([workspace.package]); every other manifest that
// carries one must agree. The .NET packages read Cargo.toml directly, so they cannot drift.
// Prints the version on success; exits 1 listing the manifests that disagree.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const repo = join(import.meta.dirname, '..');
const read = (relative: string): string => readFileSync(join(repo, relative), 'utf8');

const cargoVersion = /^version = "([^"]+)"/m.exec(read('rust/Cargo.toml'))?.[1];
if (!cargoVersion) {
  console.error('No [workspace.package] version in rust/Cargo.toml');
  process.exit(1);
}

const manifests = ['packages/vscode/package.json', 'plugin/.claude-plugin/plugin.json'];
const mismatches = manifests.flatMap((path) => {
  const { version } = JSON.parse(read(path)) as { version: string };
  return version === cargoVersion ? [] : [`${path} says ${version}`];
});

if (mismatches.length > 0) {
  console.error(`rust/Cargo.toml says ${cargoVersion}, but:\n  ${mismatches.join('\n  ')}`);
  process.exit(1);
}
console.log(cargoVersion);
