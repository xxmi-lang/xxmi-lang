# Rust port

`rust/` ports the TypeScript core, CLI and language server: `tree-sitter-migoto` (C parser via `cc`, grammar commit e0df4ec), `xxmi-core` (spec, model, workspace, 19 rules, lint engine) and `xxmi` (the binary: `lint`, `query`, `index`, `lsp`).
The TypeScript implementation stays the reference. The release pipeline is not ported; the MCP server is (`xxmi-mcp`, below).

## Ported

- `xxmi lint`, `query` and `index` keep the flags, text/JSON output and exit codes of `packages/cli`; `lint` adds a runtime `--spec <path>` override.
- `xxmi lsp [--stdio]` (also `xxmi-lsp`, or a bare `--stdio`) has the capabilities and result shapes of `packages/lsp`: diagnostics with a 50 ms leading-edge debounce, quick fixes, definition, references, rename, hover, completion, document symbols and watched files.
  Edits reparse only the changed sections of the open document.
- `build.rs` embeds `packages/core/dist/spec.json` and `snapshots/zzmi.json`, so run `pnpm build` first.

- `xxmi-spec-extractor` ports `packages/spec-extractor` and the overlay merge of `packages/core/src/spec/load.ts`. It is a library plus its own binary, `xxmi-spec-extract` (not part of `xxmi` and not shipped in releases), with the flags and exit codes of `pnpm spec:extract`. Extra flags: `--full <file>` writes the overlay-merged spec as compact JSON (`--spec-root <dir>`, default `spec`), and `--dll-commit`, `--dll-commit-date`, `--dll-repository` (and `--baseline-*`) stand in for git metadata when a source is not a checkout.
- `xxmi-mcp` ports `packages/mcp-server`: the nine tools and the HTTP surface (`/mcp`, `/healthz`, `/admin/refresh`), the git-based indexer with per-DLL-version spec builds (via `xxmi-spec-extractor`), and the same volume layout and environment variables, so either server can run on the other's `/data`. It uses the same `builder` stamp scheme, so each rebuilds the other's specs once. How to run it: `docs/06-mcp-server.md` (Rust server).

## Build and verify

```bash
cd rust && cargo build --release && cargo clippy --release -- -D warnings && cargo test --release
node rust/bench/parity.mjs        # lint, query, index against the TS CLI
node rust/bench/lsp-parity.mjs    # starts the TS and the Rust server on the same sessions
node rust/bench/lsp-latency.mjs   # XXMI_TRACE_TIMING=1 adds the per-span breakdown of an edit
node rust/bench/spec-parity.mjs   # spec extractor: TS against Rust on the DLL at master and five tags
node rust/bench/mcp-parity.mjs    # MCP server: TS against Rust, fixture volume then local-ref indexing; ends with scripts/mcp-acceptance.ts
node rust/bench/mcp-load.mjs      # req/s, latency and RSS of both servers
```

`cargo test` runs the rule goldens (`fixtures/rules`, `fixtures/xxmi-1.3.1`, quick fixes against `fixed.ini`), the LSP protocol session, and the robustness suites (core fuzz, hostile LSP input).

## Parity

- `parity.mjs`: fixtures 85 files, 178 diagnostics, and ZZMI 19 files, 17 diagnostics; 0 mismatches, byte-identical output.
- `lsp-parity.mjs`: 31,945 responses (documentSymbol, hover, definition, references, completion, rename, codeAction, published diagnostics after open, edit and close), 0 mismatches; it exits 1 on any difference.

- `spec-parity.mjs`: XXMI-Libs-Package at `master`, v1.3.2, v1.3.1, v1.2.3, v1.2.2 and v1.2.0 (git archive), baseline 3Dmigoto head: every `spec/generated/*.json` byte-identical to the TS output, and the Rust spec merged with `spec/overlay` identical to `packages/core/dist/spec.json` for master; 0 differences. The fixtures in `fixtures/spec-extractor` match their goldens.
- `mcp-parity.mjs`: 91 requests on a fixture volume (every tool with good and bad arguments, the HTTP edge cases) and 15 more after both servers index the local refs in `../xxmi-refs` (GitHub URLs rewritten to them with git `insteadOf`), 0 mismatches after dropping timestamps. `scripts/mcp-acceptance.ts` passes 6/6 against the Rust server. Rate limiting is switched off for the run (a token bucket is timing-dependent; both servers' HTTP tests cover it), and `search_source` is compared on one file because ripgrep lists files in a run-dependent order in both servers.

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

Spec extraction at `master` (both DLL and baseline, overlays merged; wall time and peak RSS from `/usr/bin/time`): TS 0.49 s, 274 MB; Rust 0.28 s, 13 MB.

`lint` of ZZMI takes about 0.05 s against 0.15-0.2 s for the TS CLI.
What is left of an edit is the workspace-wide symbol table and usage (2.8 ms) and the per-file rules, which run on every edit.

### MCP server (`mcp-load.mjs 10 32`: 32 connections for 10 s on a fixture volume, mix of `lint_ini` on 200 lines, a `spec_lookup` and `/healthz`)

| | TS | Rust |
|---|---|---|
| requests per second | 600 | 5,500 |
| p50 / p99 | 40 / 126 ms | 2.2 / 90 ms |
| RSS idle | 119 MB | 6 MB |
| RSS peak under load | 119 MB | 34-39 MB |
| RSS peak while indexing the local refs and building the DLL specs | 119 MB | 47 MB |
| Docker image (`Dockerfile.rust`, Debian trixie-slim with git and ripgrep) | | 311 MB |

The image size is dominated by Debian, git and ripgrep, not by the 6 MB binary.

## Intentional differences

None visible in results: the stderr text of `query keys <unknown>` (exit code 3 is the same); the `xxmi.toml` error text and ambiguous-option wording come from the `toml` and argument-parser crates (exit code 2 is the same); `localeCompare` ordering is approximated rather than ICU; `reload` ignores include errors; `didOpen` does one update where TS does two (same final state); a panic in the language server is caught per request (an `InternalError` reply) or per notification (the document is rebuilt from its last text), where TS lets the handler throw, and a panic in the CLI exits 3; JSON key order differs (compared sorted).

MCP server: the `builder` id is `rust-<version>+extractor<N>+overlay<hash>`, so a volume shared with the TS server has its specs rebuilt once by the other; release tags are ordered with the same ICU approximation as above; a spec build that times out (120 s) leaves its thread running until it ends, and holds the single build permit meanwhile, where TS terminates its worker. Numeric settings are read as plain integers, so `1e3` and `0x10` are rejected where the TypeScript server's `Number()` accepts them.
