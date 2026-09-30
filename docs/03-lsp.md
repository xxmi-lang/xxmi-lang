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
