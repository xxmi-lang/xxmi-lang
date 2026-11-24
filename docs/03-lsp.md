# 03 — LSP server (`xxmi-lsp`)

A stdio language server written in Rust (`rust/crates/xxmi`). It is a single-file binary run as `xxmi-lsp` or `xxmi lsp --stdio` by Kate, Claude Code and VS Code.

## Capabilities by phase

**Phase 1 (MVP, needed by the first release):**

| Capability                              | Behavior                                                                                                                                                                                                                                                             |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `textDocument/publishDiagnostics`       | All enabled rules. Debounce 50 ms after the last change. Cross-file diagnostics (XM2xx) refresh when dependencies change                                                                                                                                             |
| `textDocument/definition`               | Sections, variables, resources, pools, custom shaders, include targets, `filename =` paths (opens the file), and library symbols in the package or bundled snapshot                                                                                                  |
| `textDocument/references`               | All references across the indexed workspace, including the package                                                                                                                                                                                                   |
| `textDocument/rename` + `prepareRename` | Rename sections (updates every `CommandList\N\X`, `ref ResourceX`, `run = …` form), variables (`$x`, `$\N\x`), resources. Refuse renames of symbols in the package or bundled snapshot, with a clear message                                                         |
| `textDocument/hover`                    | Spec docs for keys, commands and operators (with an "XXMI-only" badge where it applies); doc comments for user and library symbols; resolved value for `[Constants]` literals                                                                                        |
| `textDocument/completion`               | Section prefixes after `[`; keys valid for the current section; commands in command lists; `$` variables in scope and cross-namespace; `CommandList\`, `Resource\` paths with namespace segments; enum values; DXGI formats; file paths for `filename =`/`include =` |
| `textDocument/documentSymbol`           | Outline: sections grouped by kind                                                                                                                                                                                                                                    |
| `textDocument/codeAction`               | Quick fixes from `02-diagnostics.md`                                                                                                                                                                                                                                 |

**Phase 2:**

- `textDocument/semanticTokens/full` (+ delta): namespaces, resolved vs unresolved references, library symbols, locked globals. Token types map onto lupomikti's TextMate scopes so themes agree (see `08-editor-integration.md`).
- `workspace/symbol`: fuzzy search across the mod and the package.
- `textDocument/foldingRange`: sections, if-blocks, doc-comment blocks.
- `textDocument/inlayHint`: resolved values of `[Constants]` in expressions; namespace of an unqualified reference when ambiguous.
- `textDocument/signatureHelp` for commands with positional args (`store`, draw commands).

### As implemented (M3)

The Rust server (`xxmi-lsp`) implements phase 1 without formatting: that moved to the maintainer's C# app (M4, `09-roadmap.md`), and the server only informs. Navigation, hover and completion logic is in `xxmi-core` (`rust/crates/xxmi-core/src/navigation/`); the server only keeps editor state and converts types.

- **Diagnostics:** every open document is re-linted after any change, so cross-file results stay current. The debounce has a leading edge: a change after one debounce period of quiet (50 ms) is linted at once (an agent's single edit gets diagnostics immediately), and a burst of keystrokes is linted one period after the last one.
- **Definition, references, rename:** sections, variables (`$x`, `$\N\x`, locals), resources, pools, include targets and `filename =` paths. Rename edits only the name part of each reference and keeps prefixes and namespaces as written. It refuses symbols from the bundled snapshot, from the DLL's built-in sections, and from the package (`Core/`, d3dx.ini).
- **Index:** open files first, then every `.ini` under the workspace folders in the background (stops after 5,000 files with a warning, skips `exclude_recursive` matches such as `DISABLED*` folders), then the package root's d3dx.ini and its includes. Files created later (`workspace/didChangeWatchedFiles`, which the VS Code extension sends for `**/*.{ini,toml}`) join the index when the startup index would have taken them; deleted files leave it unless open. One notification is one batch with a single refresh. The server registers no file watchers itself: other editors must send `workspace/didChangeWatchedFiles` for `**/*.{ini,toml}`, or restart the server.
- **Hover:** spec docs (generated + overlays) with an XXMI-only badge, the doc comment above a section (or the comment block right below its header, ZZMI's `; Input:` style), and a variable's declaration line.
- **Completion:** section kinds after `[`; keys, commands, flow keywords (`if`, `elif`, `else`, `endif`) and declarations (`local`, `global`) at the start of a line, inserting ` = ` (or a space after a keyword) unless the line already has `=`; `$` variables anywhere, including conditions; enum values, DXGI formats, command values, references (own-namespace names short, offered as soon as a prefix like `Res` is typed) and file paths after `=`; resources and pools for resource copies (`ps-t0 = `). Nothing in comments. The VS Code extension turns off word-based suggestions for `migoto`, so an empty result shows nothing instead of words from the file.
- **Work queue:** document updates, saves and indexing run one at a time; requests wait for it. A failure is logged and never stops the queue or the process.
- **Code actions:** "did you mean" quick fixes (XM102, XM105, XM201, XM202).
- **Settings:** `xxmi.toml`, plus `workspace/configuration` section `xxmi` (`rules`, `miPackage`), which wins. `miPackage` sets the game profile; see `01-language-model.md` §5.

Measured with `rust/bench/lsp-latency.mjs` on an Apple M3 Max, with a 5,048-line document next to ZZMI:

| Target                                             | Measured                                        |
| -------------------------------------------------- | ----------------------------------------------- |
| Cold index of the ZZMI package plus one mod: < 1 s | 0.1-0.4 s                                       |
| Edit → diagnostics on a 5,048-line file: < 100 ms  | median 12.1 ms, p95 15.7 ms, max 17.2 ms        |
| Package plus 50 mods: < 200 MB                     | RSS 15.0 MB after open, 19.0 MB after 100 edits |

Parse and lowering take 2.9 ms and lint 7.3 ms for that file.

## Performance targets

Measure on the largest file in `corpus/` and on the whole ZZMI package.

- Cold index of the ZZMI package plus one mod: < 1 s.
- Keystroke to diagnostics on a 5,000-line file: < 100 ms after the debounce.
- Memory: < 200 MB for the package plus 50 mods open.

## Robustness

- Never crash on input. Catch per-request, log, and return an empty result.
- Files over 5 MB (some CN mods are huge flat files): full parse, but only syntax rules run live; cross-file rules run on save.
- Settings are read from `xxmi.toml` and `workspace/configuration` (section `xxmi`). The client setting overrides the file.
- `xxmi.toml` is read once per folder and cached: editing it needs a server restart to take effect.
- A broken `xxmi.toml` (unparsable, unknown `profile`, invalid rule value) is logged once with `window/logMessage`, and files under it are linted with the default settings and profile detection. The CLI still exits with a usage error (`04-cli.md`).

## Testing

- Unit: core functions, called directly (`cargo test` in `rust/`).
- Protocol: `tests/lsp-protocol` drives the server over JSON-RPC (`pnpm test:lsp`, with `XXMI_LSP_COMMAND` pointing at the binary).
- Smoke test in CI: spawn the compiled binary, `initialize`, open a fixture, assert diagnostics, `shutdown`.

As built: `tests/lsp-protocol` drives the server over JSON-RPC (diagnostics after an edit, quick fix, definition into a library, cross-file references and rename, refused library rename, hover, completion, outline, unknown documents). `XXMI_LSP_COMMAND` points the suite at the server binary.
