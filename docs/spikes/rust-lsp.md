# Spike: the language server in Rust

Question: is porting the TypeScript core, LSP and CLI (later the MCP server) to Rust worth it?
This spike answers with numbers; it is not a port. Code: `rust/` (about 2,300 lines of Rust).
Machine: Apple M3 Max, 64 GB, macOS, rustc 1.100.0, TS side Node 24 and the stale v0.1.0 `dist/bin` binary.

## What was built

- `tree-sitter-migoto`: the fork's generated `parser.c` and `scanner.c` (commit 4ca9e48) compiled by `cc`; native, no WASM.
- `xxmi-core`: spec (bundled `packages/core/dist/spec.json`, embedded at build; `pnpm build` first), documents (per-section trees, incremental re-parse, UTF-16 positions), scan/model lowering, XM001 and XM102 with the did-you-mean fix.
- `xxmi-rs`: `lint <path>... --format json [--rules]`, `lsp --stdio` (`lsp-server` 0.10 plus `gen-lsp-types` 0.11, which worked, so `lsp-types` was not needed), `--version`.
- No debounce: every change is linted and published at once. Only open, change, close, shutdown and diagnostics exist.
- Tests: `cargo test` (conformance on `fixtures/` against the golden JSON filtered to XM001/XM102, incremental edits incl. UTF-16, multi-line and CRLF). fmt and clippy are clean.
- Benches: `rust/bench/lsp-latency.mjs` (LSP driver) and `cargo run --release -p xxmi-core --example timing`.

## Measurements (Rust spike vs TypeScript)

| | Rust `opt-level="s"` | Rust `opt-level=3` | TypeScript |
|---|---|---|---|
| a. binary size | 1,479,536 B (gzip 449,940) | 1,628,064 B (gzip 519,278) | Bun binary, 62 MB (64998242 B, stale v0.1.0) |
| b. `--version`, median / min of 30 | 67 / 17 ms | 84 / 18 ms | 110 / 49 ms (binary); 678 ms (`pnpm xxmi`, 10 runs) |
| b. lint one small ini, median / min | 92 / 23 ms | 78 / 15 ms | 136 / 91 ms (binary); 720 ms (`pnpm xxmi`) |
| d. first diagnostics after `didOpen` (5,048 lines) | 14.6 ms | 10.1 ms | cold index 83-92 ms (includes loading the ZZMI package) |
| d. `didChange` to `publishDiagnostics`, median / p95 / max | 5.6 / 9.1 / 9.9 ms | 5.0 / 7.7 / 8.4 ms | 37-46 ms median (perf-lsp, see below) |
| d. full parse / incremental re-parse / lint (in process) | 7.1 / 0.16 / 2.5 ms | not measured | n/a |
| d. RSS idle after open / peak | 8.6 / 12.2 MB | 8.7 / 11.9 MB | 148 MB (package + 51 mods) |
| e. clean release build | 14 s (26 s CPU) | 15 s | `pnpm build` |
| e. third-party crates | 42 (unique, `cargo tree`) | same | n/a |
| e. external scanner | yes (`scanner.c`, 39 KB) | | |

How to read these:
- Process-spawn time on this machine is bimodal. `/usr/bin/true` measured 96 ms median (26 ms min) from Python, so the medians in rows b are mostly spawn noise. Trust the min column and the ratio, not the absolute median.
- `pnpm xxmi` is from source and also pays Node start-up and type stripping.
- The TS binary embeds a runtime and the whole feature set (19 rules, workspace, navigation), so size and start-up are not like for like with a 2-rule spike.
- Edit latency: `scripts/perf-lsp.ts` calls `workspace.openDocument` (full re-parse; the TS code never edits the old tree, checked by grep) and `lintFile` (19 rules, workspace re-index) directly. It has no debounce and no transport. The server (`packages/lsp/src/server.ts`) adds a 150 ms debounce with a leading edge: an isolated edit is linted at once, a burst waits 150 ms. Rerun today: 83 ms cold, 44 ms edit, 148 MB (with `--liftoff-only`).
- Rust does 2 rules with no cross-file resolution and no package index, and includes JSON-RPC over a pipe. So Rust is faster, but part of the gap is work it does not do; the remaining 17 rules are reference rules that need the workspace and would cost more. The parse work is the comparable part: 7 ms full and 0.16 ms incremental against a full re-parse in TS.
- RSS is for one open document and no package, so it is not comparable with 148 MB; it only shows the floor.

## Conformance (XM001 and XM102, id + range + message)

```
fixtures/                         86 files (TS 85)   16 diagnostics   16 match   0 mismatch
xxmi-refs/ZZMI-Package/ZZMI       19 files           15               15         0
xxmi-refs/3Dmigoto                 4 files           11               11         0
xxmi-refs/tree-sitter-migoto/examples  3 files       11               11         0
xxmi-refs/property-access-fix      2 files            2                2         0
```

114 files, 55 diagnostics, all identical. The one file-count difference: the TS CLI skips `DISABLED*` per the package's `exclude_recursive`; `xxmi-rs lint` walks every `.ini`. That file has no diagnostics.
Known model differences (not triggered by the corpus): XM001 skips expression lines like TS but XM109, which reports them, is not ported; sections are parsed as separate chunks like TS, but only the statement kinds needed for XM102 are recorded; inline suppression (`lint/suppress.ts`), config and `--fix` are not ported (fix edits are produced and tested).
The wasm grammar in TS is commit 5d78a72, the Rust one 4ca9e48 (a descendant); no difference showed up.

## Porting the rest

Not covered, from `packages/core/src` (5,020 lines in total): navigation 839, query 949, workspace 582 (file graph, symbols, snapshots), the 17 other rules (references.ts 234, rest of structure.ts), lint engine/suppress/config/format 450, `paths`, `defaults`, `runtime`. Covered by the spike: scan, lower (partly), spec lookup, parser issues, suggest.
Later stages: `packages/lsp` 455, `packages/cli` 382, `packages/spec-extractor` 1,347 (parses the DLL's C++ with regexes; good Rust fit but the TS version is the reference), `packages/mcp-server` 2,297 (HTTP, git, GitHub, store; the biggest and the least CPU-bound, so the weakest case for Rust).

Awkward to port:
- Everything else that consumes the TS packages: the VS Code extension bundles the TS server, the .NET library shells out to `xxmi`. A Rust binary can replace both, but two implementations must coexist until parity.
- JS string semantics: UTF-16 columns and edit distance (done here), `toLowerCase` versus Rust Unicode lowercasing, sort order of suggestions.
- The spec build is TS (`pnpm build`); `xxmi-core/build.rs` depends on its output, so the Rust build is not self-contained until the spec bundling moves too.

## Risks

- `gen-lsp-types` is new (0.11) with a small user base; `lsp-server` is rust-analyzer's and solid. A fallback to `lsp-types` exists.
- Two implementations drifting: needs a shared golden suite, which the repo's fixtures already provide.
- Performance targets in docs/03 are already met by TS; Rust buys start-up (about 3-5x on the min) and memory, not feasibility.
- Cross-compilation: the C grammar needs a C toolchain per target (`cc`; MSVC flag `-utf-8` is set, untested here).
- Timings are on one machine with noisy process spawn.
