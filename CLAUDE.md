# xxmi-lang — working notes for Claude Code

Language tooling for 3DMigoto / XXMI mod `.ini` files: a shared core, an LSP server, a CLI (`xxmi`), a hosted MCP server, and agent skills. ZZZ (ZZMI) is the first-class target; GIMI/SRMI/WWMI/HIMI follow through the same "game profile" mechanism.

`xxmi-lang` and `xxmi` are working names. Rename freely, but keep them consistent across packages.

## Read before working

| Doc | When |
|---|---|
| `docs/00-overview.md` | Always, first. Goals, non-goals, stack, repo layout |
| `docs/01-language-model.md` | Anything touching parsing, spec, symbols, namespaces, indexing |
| `docs/02-diagnostics.md` | Adding or changing a lint rule |
| `docs/03-lsp.md` | LSP features, performance, protocol behavior |
| `docs/04-cli.md` | CLI commands, output formats, exit codes |
| `docs/05-transforms.md` | `fmt`, `normalize` (CN cleanup), `migrate` (format eras) |
| `docs/06-mcp-server.md` | Hosted MCP server on Railway |
| `docs/07-agent-integration.md` | Claude Code plugin, skills, other agents |
| `docs/08-editor-integration.md` | VS Code, Kate, HLSL, lupomikti compatibility |
| `docs/09-roadmap.md` | What to build next, milestones, acceptance criteria |

## Ground truth, in priority order

1. **XXMI DLL source** (`SpectrumQT/XXMI-Libs-Package`, `DirectX11/IniHandler.cpp`, `DirectX11/CommandList.cpp`). If a doc and the DLL disagree, the DLL wins. Open an issue in this repo noting the mismatch.
2. **The game package** (for ZZZ: `leotorrez/ZZMI-Package`, `ZZMI/Core/`). This defines what libraries exist and what they export.
3. **Our fork of tree-sitter-migoto** (based on `lupomikti/tree-sitter-migoto`). This is the parser. Where it lags the XXMI dialect, fix it in our fork (see `docs/01-language-model.md` §2). We don't send PRs to lupomikti's repos, except Kate syntax updates (see `docs/08-editor-integration.md`).
4. These docs.

## Hard rules

- **Never guess the language.** Every keyword, section prefix, command and operator in `spec/` must trace to a DLL source location or be marked `"source": "manual"` with a reason.
- **One core, many frontends.** LSP, CLI and MCP call `@xxmi-lang/core`. No parsing, resolution or rule logic lives in a frontend.
- **Tolerant by default.** Real mods are messy. A parse error in one section must not hide diagnostics in others, and must never crash the server.
- **Transforms are lossless unless told otherwise.** `fmt`, `normalize` and `migrate` preserve comments and unknown content, and must round-trip: formatting already-formatted output is a no-op.
- **Golden tests for every transform and rule.** Fixture in, expected out, checked in. No rule merges without a positive and a negative fixture.
- **Don't vendor mods.** Test fixtures are minimal hand-written snippets or files from repos whose license allows it. Real community mods (especially CN mods) go in the local-only `corpus/` directory, which is git-ignored (see `docs/05-transforms.md`).
- **Stay compatible with lupomikti's editor work.** VS Code language id `migoto`, Kate syntax name `Migoto`. We add LSP features on top; we don't ship competing grammars.
- **License:** GPL-3.0-or-later, matching the ecosystem. tree-sitter-migoto is MIT, which is compatible.

## Commands

```bash
pnpm install
pnpm build               # all packages
pnpm test                # unit + golden tests
pnpm test:corpus         # parses corpus/ (local only) + $XXMI_PACKAGE_SRC; failure rate, first error per file, baseline diff (lint from M2)
pnpm grammar:update      # rebuilds packages/core/grammar/*.wasm from a tree-sitter-migoto fork checkout (XXMI_GRAMMAR_SRC env var)
pnpm spec:extract        # regenerates spec/generated/*.json (XXMI_DLL_SRC = XXMI-Libs-Package, MIGOTO_DLL_SRC = bo3b/3Dmigoto for xxmi flags); --check to verify
pnpm xxmi lint path/to/mod                                 # CLI from source (paths relative to the cwd)
pnpm xxmi index ../ZZMI-Package/ZZMI --out packages/core/snapshots/zzmi.json --name zzmi   # rebuild the bundled library snapshot
```

## Definition of done for any task

- Tests added and passing (`pnpm test`).
- `pnpm lint && pnpm typecheck` clean.
- If behavior visible to users changed, update the relevant `docs/` file in the same PR.
- If a spec entry was added by hand, it has `"source": "manual"` and a `"note"`.
