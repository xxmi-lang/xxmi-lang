/**
 * Measures the docs/03-lsp.md performance targets against a package checkout:
 *
 *   XXMI_PACKAGE_SRC=../ZZMI-Package/ZZMI node --expose-gc scripts/perf-lsp.ts
 *
 * - cold index of the package plus one mod            target < 1 s
 * - re-lint after an edit to a 5,000-line file         target < 100 ms (after the debounce)
 * - memory for the package plus 50 open mods           target < 200 MB
 *
 * Synthetic mods are written to a temp folder inside a copy of nothing: they reference the
 * package's libraries by namespace, like real mods.
 */
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  defaultSpec,
  lintFile,
  SpecLookup,
  Workspace,
  bundledSnapshots,
} from '../packages/core/src/index.ts';

const pkg = process.env.XXMI_PACKAGE_SRC;
if (!pkg) {
  console.error('Set XXMI_PACKAGE_SRC to a package folder (the one with d3dx.ini).');
  process.exit(2);
}

function modText(n: number, sections: number): string {
  const out: string[] = [`namespace = PerfMod${n}`, ''];
  for (let i = 0; i < sections; i++) {
    out.push(
      `[TextureOverrideComponent${i}]`,
      `hash = ${(0x10000000 + i).toString(16)}`,
      'match_first_index = 0',
      'handling = skip',
      `ib = ResourceIndex${i}`,
      'if $active && DRAW_TYPE == 1',
      `\tps-t0 = ResourceDiffuse${i}`,
      `\trun = CommandListShared`,
      'endif',
      'drawindexed = auto',
      '',
      `[ResourceIndex${i}]`,
      'type = Buffer',
      'format = DXGI_FORMAT_R32_UINT',
      '',
      `[ResourceDiffuse${i}]`,
      'filename = diffuse.dds',
      '',
    );
  }
  out.push(
    '[Constants]',
    'global $active = 1',
    '',
    '[CommandListShared]',
    'run = CommandList\\ZZMI\\SetTextures',
    '',
  );
  return out.join('\n');
}

const mb = (bytes: number): string => `${(bytes / 1024 / 1024).toFixed(0)} MB`;
const gc = (globalThis as { gc?: () => void }).gc;
const heapBefore = (gc?.(), process.memoryUsage());

const lookup = new SpecLookup(defaultSpec());
const workspace = new Workspace({ lookup, snapshots: bundledSnapshots() });
const root = resolve(pkg);
const temp = mkdtempSync(join(tmpdir(), 'xxmi-perf-'));

// 1. Cold index: the package plus one mod.
let t = performance.now();
await workspace.loadPackage(root);
const first = join(temp, 'Mod0', 'mod.ini');
mkdirSync(join(temp, 'Mod0'));
writeFileSync(first, modText(0, 20));
const firstFile = await workspace.addTarget(first, temp);
lintFile(firstFile, workspace);
const coldMs = performance.now() - t;

// 2. Memory: package + 50 open mods (measured before the big-file churn below, which leaves V8
// holding freed heap pages for a while).
for (let n = 1; n <= 50; n++) {
  const text = modText(n, 20);
  await workspace.openDocument(join(temp, `Mod${n}`, 'mod.ini'), text, temp);
}
for (const file of workspace.files.values()) lintFile(file, workspace);
gc?.();
const heap = process.memoryUsage();

// 3. Edit → diagnostics on a 5,000-line file.
const big = modText(999, 280);
const bigPath = join(temp, 'big.ini');
writeFileSync(bigPath, big);
await workspace.openDocument(bigPath, big, temp);
const lines = big.split('\n').length;
const samples: number[] = [];
for (let i = 0; i < 10; i++) {
  const edited = big.replace('handling = skip', `handling = skip\n; edit ${i}`);
  t = performance.now();
  const file = await workspace.openDocument(bigPath, edited, temp);
  lintFile(file, workspace);
  samples.push(performance.now() - t);
}
samples.sort((a, b) => a - b);
const editMs = samples[Math.floor(samples.length / 2)] ?? 0;

const afterChurn = (gc?.(), process.memoryUsage());

const rows = [
  ['cold index (package + 1 mod)', `${coldMs.toFixed(0)} ms`, '< 1000 ms', coldMs < 1000],
  [
    `edit → diagnostics (${lines} lines, median of 10)`,
    `${editMs.toFixed(0)} ms`,
    '< 100 ms',
    editMs < 100,
  ],
  [
    `memory (package + 51 mods, ${workspace.files.size} files, RSS)`,
    mb(heap.rss),
    '< 200 MB',
    heap.rss < 200 * 1024 * 1024,
  ],
] as const;
for (const [what, value, target, ok] of rows)
  console.log(`${ok ? 'ok  ' : 'FAIL'}  ${what}: ${value} (target ${target})`);
console.log(
  `after the 5,000-line edits: rss ${mb(afterChurn.rss)} (V8 releases freed pages lazily)`,
);
console.log(
  `breakdown: rss ${mb(heap.rss)}, heap used ${mb(heap.heapUsed)} (+${mb(heap.heapUsed - heapBefore.heapUsed)}), heap total ${mb(heap.heapTotal)}, external ${mb(heap.external)}, start rss ${mb(heapBefore.rss)}${gc ? '' : ' (run with --expose-gc for stable numbers)'}`,
);
process.exit(rows.every(([, , , ok]) => ok) ? 0 : 1);
