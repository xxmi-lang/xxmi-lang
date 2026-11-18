# Rust port

`rust/` ports the TypeScript core, CLI and language server: `tree-sitter-migoto` (C parser via `cc`, grammar commit f308702), `xxmi-core` (spec, model, workspace, 19 rules, lint engine) and `xxmi` (the binary: `lint`, `query`, `index`, `lsp`).
The TypeScript implementation stays the reference. The release pipeline and the MCP server are not ported.

## Ported

- `xxmi lint`, `query` and `index` keep the flags, text/JSON output and exit codes of `packages/cli`; `lint` adds a runtime `--spec <path>` override.
- `xxmi lsp [--stdio]` (also `xxmi-lsp`, or a bare `--stdio`) has the capabilities and result shapes of `packages/lsp`: diagnostics with a 50 ms leading-edge debounce, quick fixes, definition, references, rename, hover, completion, document symbols and watched files.
  Edits reparse only the changed sections of the open document.
- `build.rs` embeds `packages/core/dist/spec.json` and `snapshots/zzmi.json`, so run `pnpm build` first.

## Build and verify

```bash
cd rust && cargo build --release && cargo clippy --release -- -D warnings && cargo test --release
node rust/bench/parity.mjs        # lint, query, index against the TS CLI
node rust/bench/lsp-parity.mjs    # starts the TS and the Rust server on the same sessions
node rust/bench/lsp-latency.mjs   # XXMI_TRACE_TIMING=1 adds the per-span breakdown of an edit
```

`cargo test` runs the rule goldens (`fixtures/rules`, `fixtures/xxmi-1.3.1`, quick fixes against `fixed.ini`), the LSP protocol session, and the robustness suites (core fuzz, hostile LSP input).

## Parity

- `parity.mjs`: fixtures 85 files, 178 diagnostics, and ZZMI 19 files, 17 diagnostics; 0 mismatches, byte-identical output.
- `lsp-parity.mjs`: 31,945 responses (documentSymbol, hover, definition, references, completion, rename, codeAction, published diagnostics after open, edit and close), 0 mismatches; it exits 1 on any difference.

## Speed and size (Apple M3 Max; 5,048-line document next to ZZMI)

| | Before | After | Target |
|---|---|---|---|
| edit to diagnostics, median / p95 / max | 38.5 / 44.9 / 49.3 ms | 12.1 / 15.7 / 17.2 ms | < 100 ms |
| of which: parse + lowering | 25.3 ms | 2.9 ms | |
| of which: lint (19 rules) | 11.6 ms | 7.3 ms | |
| RSS after open / after 100 edits | 14.5 / 16.4 MB | 15.0 / 19.0 MB | < 200 MB |
| cold start to first diagnostics | 0.1-0.3 s | 0.1-0.4 s (noisy) | < 1 s |
| binary size | 2,275,760 B | 2,292,368 B | |
| binary size with `panic = "unwind"` | 2,292,368 B | 2,573,536 B (+12%) | |
| TS binary (stale v0.1.0 Bun) | 64,998,242 B | | |

`lint` of ZZMI takes about 0.05 s against 0.15-0.2 s for the TS CLI.
What is left of an edit is the workspace-wide symbol table and usage (2.8 ms) and the per-file rules, which run on every edit.

## Intentional differences

None visible in results: the stderr text of `query keys <unknown>` (exit code 3 is the same); the `xxmi.toml` error text and ambiguous-option wording come from the `toml` and argument-parser crates (exit code 2 is the same); `localeCompare` ordering is approximated rather than ICU; `reload` ignores include errors; `didOpen` does one update where TS does two (same final state); a panic in the language server is caught per request (an `InternalError` reply) or per notification (the document is rebuilt from its last text), where TS lets the handler throw, and a panic in the CLI exits 3; JSON key order differs (compared sorted).
