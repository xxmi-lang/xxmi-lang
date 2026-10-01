# 03 — LSP server (`xxmi-lsp`)

A stdio language server built on `vscode-languageserver/node`, using `@xxmi-lang/core` for everything. It ships as a Node package (for VS Code) and as a single-file binary (for Kate and Claude Code).

## Capabilities by phase

**Phase 1 (MVP, needed by the first release):**

| Capability | Behavior |
|---|---|
| `textDocument/publishDiagnostics` | All enabled rules. Debounce 150 ms after the last change. Cross-file diagnostics (XM2xx) refresh when dependencies change |
| `textDocument/definition` | Sections, variables, resources, pools, custom shaders, include targets, `filename =` paths (opens the file), and library symbols in the package or bundled snapshot |
| `textDocument/references` | All references across the indexed workspace, including the package |
| `textDocument/rename` + `prepareRename` | Rename sections (updates every `CommandList\N\X`, `ref ResourceX`, `run = …` form), variables (`$x`, `$\N\x`), resources. Refuse renames of symbols in the package or bundled snapshot, with a clear message |
| `textDocument/hover` | Spec docs for keys, commands and operators (with an "XXMI-only" badge where it applies); doc comments for user and library symbols; resolved value for `[Constants]` literals |
| `textDocument/completion` | Section prefixes after `[`; keys valid for the current section; commands in command lists; `$` variables in scope and cross-namespace; `CommandList\`, `Resource\` paths with namespace segments; enum values; DXGI formats; file paths for `filename =`/`include =` |
| `textDocument/documentSymbol` | Outline: sections grouped by kind |
| `textDocument/formatting` | `xxmi fmt` rules (see `05-transforms.md`) |
| `textDocument/codeAction` | Quick fixes from `02-diagnostics.md` |

**Phase 2:**

- `textDocument/semanticTokens/full` (+ delta): namespaces, resolved vs unresolved references, library symbols, locked globals. Token types map onto lupomikti's TextMate scopes so themes agree (see `08-editor-integration.md`).
- `workspace/symbol`: fuzzy search across the mod and the package.
- `textDocument/foldingRange`: sections, if-blocks, doc-comment blocks.
- `textDocument/inlayHint`: resolved values of `[Constants]` in expressions; namespace of an unqualified reference when ambiguous.
- `textDocument/signatureHelp` for commands with positional args (`store`, draw commands).
- Custom command `xxmi/migrate` and `xxmi/normalize` exposed as code actions over the whole file ("Migrate this mod to blend-v2…").

### As implemented (M3)

`packages/lsp` (`xxmi-lsp`) implements phase 1 except formatting, which waits for `xxmi fmt` (M4). Navigation, hover and completion logic is in `@xxmi-lang/core` (`src/navigation/`); the server only keeps editor state and converts types.

- **Diagnostics:** every open document is re-linted after any change, so cross-file results stay current. The debounce has a leading edge: a change after 150 ms of quiet is linted at once (an agent's single edit gets diagnostics immediately), and a burst of keystrokes is linted 150 ms after the last one.
- **Definition, references, rename:** sections, variables (`$x`, `$\N\x`, locals), resources, pools, include targets and `filename =` paths. Rename edits only the name part of each reference and keeps prefixes and namespaces as written. It refuses symbols from the bundled snapshot, from the DLL's built-in sections, and from the package (`Core/`, d3dx.ini).
- **Index:** open files first, then every `.ini` under the workspace folders in the background (stops after 5,000 files with a warning), then the package root's d3dx.ini and its includes.
- **Hover:** spec docs (generated + overlays) with an XXMI-only badge, the doc comment above a section (or the comment block right below its header, ZZMI's `; Input:` style), and a variable's declaration line.
- **Completion:** section kinds after `[`; keys and commands at the start of a line; enum values, DXGI formats, command values, references (own-namespace names short), `$` variables and file paths after `=`.
- **Code actions:** "did you mean" quick fixes (XM102, XM105, XM201, XM202).
- **Settings:** `xxmi.toml`, plus `workspace/configuration` section `xxmi` (`rules`), which wins.
- **Node and WASM memory:** under Node, web-tree-sitter leaks about 0.2 MB of native memory per parse once V8 tiers its WASM up to the optimizing compiler (Node 24, web-tree-sitter 0.26 and 0.27; Bun doesn't leak). Node entry points therefore relaunch themselves with `--liftoff-only` (`packages/core/src/runtime.ts`), and the VS Code client passes it directly. Parsing isn't measurably slower.

Measured with `XXMI_PACKAGE_SRC=<ZZMI-Package>/ZZMI node --expose-gc --liftoff-only scripts/perf-lsp.ts` (macOS, Node 24, 2026-10-01):

| Target | Measured |
|---|---|
| Cold index of the ZZMI package plus one mod: < 1 s | 89 ms |
| Edit → diagnostics on a 5,048-line file: < 100 ms | 44 ms (median of 10) |
| Package plus 50 mods: < 200 MB | 177 MB RSS (223 MB right after heavy edits to a 5,000-line file, until V8 returns freed pages) |

## Performance targets

Measure on the largest file in `corpus/` and on the whole ZZMI package.

- Cold index of the ZZMI package plus one mod: < 1 s.
- Keystroke to diagnostics on a 5,000-line file: < 100 ms after the debounce.
- Memory: < 200 MB for the package plus 50 mods open.

## Robustness

- Never crash on input. Catch per-request, log, and return an empty result.
- Files over 5 MB (some CN mods are huge flat files): full parse, but only syntax rules run live; cross-file rules run on save.
- Settings are read from `xxmi.toml` and `workspace/configuration` (section `xxmi`). The client setting overrides the file.

## Testing

- Unit: core functions, called directly.
- Protocol: use `vscode-languageserver-protocol` test harness (or a small JSON-RPC driver) with scripted sessions in `packages/lsp/test/sessions/*.json` (request in, expected response out).
- Smoke test in CI: spawn the compiled binary, `initialize`, open a fixture, assert diagnostics, `shutdown`.

As built: `packages/lsp/test/protocol.test.ts` drives the server over JSON-RPC (diagnostics after an edit, quick fix, definition into a library, cross-file references and rename, refused library rename, hover, completion, outline, unknown documents). `XXMI_LSP_COMMAND` points the same suite at another server: CI runs it against the compiled binary on every OS and against the bundled VS Code server. The release workflow runs it again on the downloaded release binaries.
