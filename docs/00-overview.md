# 00 — Overview

## Problem

Mod `.ini` files for XXMI (3DMigoto fork) are a real programming language: sections, command lists, conditionals, variables with namespaces, pools, expressions, custom shaders, includes. Tooling for them today is highlighting only. As a result:

- **Humans** debug by reloading the game and reading the log.
- **AI agents** read whole mods and whole libraries into context to answer "where is X defined", then still make mistakes the DLL would reject.
- **Maintaining mods is expensive:** updating mods across ZZMI format eras is manual. Applying ZZMI libraries is copy-paste. Cleaning up CN mods (flat, auto-named, often GBK-encoded) takes hours before any real change can start.

## Goals

1. **Linter and LSP first.** Diagnostics, go-to-definition, references, rename, hover and completion, in VS Code and Kate. This serves humans and agents at the same time.
2. **Mechanical transforms** (`fmt`, `normalize`, `migrate`) moved to the maintainer's C# app. `05-transforms.md` is their design reference.
3. **Hosted MCP server.** Anyone's agent can query the DLL, the ZZMI package and XXMITools without cloning them, and can lint ini text remotely.
4. **Skills.** Workflow knowledge for agents: migrate a mod, apply a library. Skills call the CLI; they don't reimplement it.

ZZZ/ZZMI is the priority profile. The design stays general so GIMI/SRMI/WWMI/HIMI profiles can be added as data.

## Non-goals (for now)

- Our own syntax highlighting grammars. We use lupomikti's TextMate and Kate grammars, and add LSP semantic tokens on top.
- An HLSL language server. Only a Kate HLSL syntax file is in scope (see `08-editor-integration.md`).
- Running the game or DLL, and frame analysis.
- Full LSP support inside XXMITools Jinja templates (`*.ini.j2`). We lint their rendered output in CI instead (see `09-roadmap.md`).
- Decrypting or bypassing protection on mods whose authors deliberately locked them.

## Users

| User                               | Surface                     | Main need                                                       |
| ---------------------------------- | --------------------------- | --------------------------------------------------------------- |
| Mod author (human)                 | VS Code, Kate               | Errors before launching the game, navigation, rename            |
| Mod fixer / maintainer             | CLI, editor                 | Whole-mod lint, navigation and rename                           |
| AI agent, local (Claude Code etc.) | LSP via plugin, CLI, skills | Diagnostics after each edit, precise lookups                    |
| AI agent, remote / no clone        | Hosted MCP                  | Spec lookups, library catalog, source search, remote lint       |
| XXMI / ZZMI maintainers            | CI                          | Lint the ZZMI package and XXMITools template output on every PR |

## Stack decisions

| Decision    | Choice                                                                                                                                                                                                                                                | Why                                                                                                                                                                        |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language    | Rust for the core, CLI, LSP and MCP server; TypeScript for the VS Code extension                                                                                                                                                                      | One static binary per platform; tree-sitter has Rust bindings; lupomikti's tooling is TS/Deno                                                                              |
| Parser      | `tree-sitter-migoto` via the `tree-sitter` crate (C sources vendored in `rust/crates/tree-sitter-migoto`, built with `cc`)                                                                                                                            | Error-tolerant and incremental. The parser is compiled into each binary                                                                                                    |
| LSP         | `vscode-languageserver` (node)                                                                                                                                                                                                                        | Standard; works for any stdio LSP client                                                                                                                                   |
| MCP         | Streamable HTTP transport, protocol implemented in `xxmi-mcp` (no MCP SDK)                                                                                                                                                                            | Remote-friendly; runs on Railway                                                                                                                                           |
| Monorepo    | one Cargo workspace (`rust/`), plus pnpm for the VS Code extension and scripts                                                                                                                                                                        | Simple                                                                                                                                                                     |
| Tests       | vitest; golden fixtures under `fixtures/`                                                                                                                                                                                                             | Fast; snapshot-friendly                                                                                                                                                    |
| Binaries    | Rust (`rust/`, cargo) for `xxmi`, with `xxmi-lsp` a copy of it, on win-x64, win-arm64, linux-x64, linux-arm64, macos-x64, macos-arm64 (`pnpm build:binaries`; the grammar, the spec and the ZZMI snapshot are embedded; Linux builds are static musl) | No runtime to install                                                                                                                                                      |
| Platforms   | Windows, Linux, macOS are all first-class for the tooling. CI runs the test suite on all three                                                                                                                                                        | The game only runs on Windows/Proton, but mods are authored, fixed and reviewed anywhere. Only the DLL itself (and the `xxmi-dll-dev` skill's build steps) is Windows-only |
| Config file | `xxmi.toml` at the mod or workspace root                                                                                                                                                                                                              | Readable; lupomikti uses TOML too                                                                                                                                          |

## Repository layout

```
xxmi-lang/
├── CLAUDE.md
├── docs/                         # these specs
├── spec/
│   ├── generated/                # output of spec:extract (committed; regenerated on DLL updates)
│   │   ├── sections.json
│   │   ├── commands.json
│   │   ├── operators.json
│   │   └── meta.json             # DLL commit hash, extraction date
│   ├── overlay/                  # hand-written docs, examples, arg types merged over generated
│   └── profiles/
│       ├── zzmi.toml             # game profile: package paths, known libraries, hash sources, format eras
│       └── gimi.toml ...         # later
├── rust/                         # Cargo workspace: xxmi-core, xxmi (CLI, LSP), xxmi-mcp, xxmi-spec-extractor, tree-sitter-migoto
├── packages/vscode/              # thin LSP client extension
├── schema/                       # lint-output.v1.json
├── snapshots/                    # bundled ZZMI library snapshot
├── scripts/                      # pnpm scripts (build:binaries, grammar:update, spec:extract, ...)
├── plugin/                       # Claude Code plugin (skills, .lsp.json, .mcp.json)
├── dotnet/                       # .NET client library + NuGet packaging (10-dotnet.md); no language logic
├── editors/                      # Neovim and Kate config, shipped in each release zip
├── fixtures/                     # golden tests (license-clean snippets only)
└── corpus/                       # git-ignored; real mods for local testing
```

## Glossary

- **DLL / XXMI DLL:** the 3DMigoto fork in XXMI-Libs-Package. It defines the language.
- **Package:** a game's XXMI package (for example ZZMI-Package), containing `d3dx.ini`, `Core/<Game>/main.ini` and `Core/<Game>/Libraries/*`.
- **Library:** a namespaced ini module inside a package (for example SlotFix, TTLib, HP bar) that mods call into.
- **Profile:** per-game data in `spec/profiles/`: package layout, libraries, format eras, hash sources.
- **Format era:** a generation of how mods are structured for a game (for ZZZ: Position-based, early Blend, current Blend). See `05-transforms.md`.
- **Namespace:** set by `namespace = X` at the top of a file, otherwise derived from the file path. References use the `\ns\name` form (`CommandList\ZZMI\SetTextures`, `$\TTL\alpha`).
