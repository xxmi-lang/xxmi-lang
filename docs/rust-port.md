# Rust port

Stage 1 of replacing the TypeScript core and CLI: `rust/` holds `tree-sitter-migoto` (C parser via `cc`, grammar commit 5d78a72), `xxmi-core` (spec, model, workspace, 19 rules, lint engine) and `xxmi` (the binary: `lint`, `query`, `index`, `lsp`).
The TypeScript implementation stays the reference. The release pipeline and MCP server are later stages.

## Stage 1 status

- `xxmi lint` keeps the flags, text/JSON output and exit codes of `packages/cli`, plus a runtime `--spec <path>` override. `build.rs` embeds `packages/core/dist/spec.json` and `snapshots/zzmi.json`, so run `pnpm build` first.
- Rule goldens: `cargo test` runs every `fixtures/rules/XMnnn/{bad,good}` and `fixtures/xxmi-1.3.1` with no snapshots, and applies quick fixes to compare with `fixed.ini`.
- Parity: `node rust/bench/parity.mjs` lints `fixtures/` and ZZMI with both CLIs and compares code, range, severity and message (and the raw JSON).
  Result: fixtures 85 files, 178 diagnostics, 0 mismatches; ZZMI 19 files, 17 diagnostics, 0 mismatches; output byte-identical.
- Known differences: the error text for a malformed `xxmi.toml` comes from the `toml` crate, and ambiguous option values are worded by the Rust argument parser (same exit code 2).

## Size and speed (Apple M3 Max)

| | Rust release | TypeScript (`dist/bin/xxmi`, stale v0.1.0 Bun binary) |
|---|---|---|
| binary size | 1,660,720 B | 64,998,242 B |
| `lint` ZZMI package (19 files), 5 runs | 0.04-0.11 s, typically 0.04 | 0.14-0.23 s |
| `lint fixtures/` (85 files) | 0.13 s | 0.34 s |

The first run of each series is a cold start; at these times process spawn dominates.

## Stage 2 status

- `xxmi query` and `xxmi index` match the TS CLI (text and `--json` output, exit codes); `index` is compared semantically.
- `xxmi lsp [--stdio]` is the language server (`lsp-server` 0.10, `gen-lsp-types` 0.11, incremental sync through `line-index`). The binary also serves LSP when invoked as `xxmi-lsp` or with a bare `--stdio`.
  It has the capabilities and result shapes of `packages/lsp`: diagnostics (150 ms leading-edge debounce, large-file rule subset, settings overrides), quick fixes, definition, references, rename with prepare, hover, completion, document symbols, watched files, and a background index that reparses only changed documents.
- Tests: `crates/xxmi/tests/lsp.rs` runs the `packages/lsp/test/protocol.test.ts` session against the binary; unit tests cover document sync and URIs.
- Parity: `node rust/bench/parity.mjs` (lint, query, index) and `node rust/bench/lsp-parity.mjs` (fixtures and ZZMI as workspaces: documentSymbol, hover, definition, references, completion, prepareRename, rename at sampled positions, codeAction, and published diagnostics after open, edit and close).
  Result: 31,945 LSP responses compared, 0 mismatches.
- Intentional differences, none visible in results: the stderr text of `query keys <unknown>` (exit code 3 is the same); `localeCompare` ordering is approximated rather than ICU; `reload` ignores include errors; `didOpen` does one update where TS does two (same final state); a panic aborts with exit 3 instead of a per-request fallback; JSON key order differs (compared sorted).

### LSP numbers (`node rust/bench/lsp-latency.mjs`, 5,048-line document next to ZZMI)

| | Rust stage 2 | docs/03-lsp.md target |
|---|---|---|
| cold start to first diagnostics (ZZMI + one mod) | 88 ms | < 1 s |
| edit to diagnostics, median / p95 / max | 38.0 / 44.8 / 46.4 ms | < 100 ms |
| RSS after open / after 100 edits | 14.0 / 16.2 MB | < 200 MB |
| binary size | 2,275,760 B | |

Edits are spaced past the debounce. The median is above the spike's 5.6 ms because every edit now runs the full 19-rule lint of the document, which the spike did not.
