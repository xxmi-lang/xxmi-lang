/**
 * Assets embedded into the single-file binaries (bun build --compile). The grammar and the
 * tree-sitter runtime are read from the binary itself, never from files next to it
 * (docs/09-roadmap.md M3); the spec and the ZZMI library snapshot are bundled as JSON.
 * Bun-only: Node can't import `.wasm` files with `type: 'file'`.
 */
import { readFileSync } from 'node:fs';
import grammarPath from '../../packages/core/grammar/tree-sitter-migoto.wasm' with { type: 'file' };
import runtimePath from '../../packages/core/node_modules/web-tree-sitter/web-tree-sitter.wasm' with { type: 'file' };
import spec from '../../packages/core/dist/spec.json' with { type: 'json' };
import zzmi from '../../packages/core/snapshots/zzmi.json' with { type: 'json' };
import {
  configureDefaults,
  initParser,
  type Spec,
  type SymbolSnapshot,
} from '../../packages/core/src/index.ts';

export async function setupEmbedded(): Promise<void> {
  configureDefaults({
    spec: spec as unknown as Spec,
    snapshots: [zzmi as unknown as SymbolSnapshot],
  });
  await initParser({
    grammarWasm: readFileSync(grammarPath),
    runtimeWasm: readFileSync(runtimePath),
  });
}
