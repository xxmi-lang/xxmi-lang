# xxmi-lang

Language tooling for **XXMI / 3DMigoto mod `.ini` files**: a linter, a language server and a
CLI. ZZZ (ZZMI) comes first; the other XXMI games (GIMI, SRMI, WWMI, HIMI) share the same core.

What you get:

- **Errors before you launch the game.** Unresolved `run =` targets and resources, unknown
  sections and keys, unbalanced `if`/`endif`, wrong argument counts, missing include and
  `filename =` files, and more. Messages quote the XXMI DLL's own warnings.
- **Navigation** in your editor: go to definition (including into the ZZMI libraries), find
  references, rename a command list or variable across files, hover docs, completion, outline,
  quick fixes.
- **For AI agents:** a Claude Code plugin that shows the agent these diagnostics after every
  edit, `xxmi query` lookups, and a hosted MCP server (not deployed yet) so an agent can ask
  about the DLL, the ZZMI libraries and lint text without cloning anything.

The language rules aren't hand-written: they're extracted from the XXMI DLL source
(`spec/generated/`), and anything XXMI added over vanilla 3DMigoto is marked as such.

## Install

**Step-by-step for Arch / CachyOS, other Linux distros, Windows and macOS:
[INSTALL.md](INSTALL.md).**

In short, download from the [latest release](https://github.com/xxmi-lang/xxmi-lang/releases/latest):

| File                             | What it is                                                                                                                                                     |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `xxmi-<version>-<os>-<arch>.zip` | `xxmi` (the CLI: `xxmi lint`, `xxmi index`, and `xxmi lsp` for the server) and `xxmi-lsp` (the language server alone, for Kate, Claude Code and other editors) |
| `xxmi-lang-<version>.vsix`       | the VS Code extension (server included)                                                                                                                        |
| `SHA256SUMS`                     | checksums of the files above                                                                                                                                   |

Targets: `windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`, `macos-x64`,
`macos-arm64`. The programs are self-contained (no Node needed). Unzip and put `xxmi` and
`xxmi-lsp` on your `PATH`; INSTALL.md has copy-paste commands for each system. The .NET
library (`XxmiLang`) is on [nuget.org](https://www.nuget.org/packages/XxmiLang) (mirrored to GitHub Packages), see [docs/10-dotnet.md](docs/10-dotnet.md).

### VS Code

Install `xxmi-lang-<version>.vsix` (Extensions view → `…` → Install from VSIX). For highlighting, also
install lupomikti's "3DMigoto INI" extension: from Open VSX in VSCodium/Code-OSS
(`AGMG.migoto-ini`), or as a `.vsix` from
[their releases](https://github.com/lupomikti/migoto-vscode/releases/latest) in Microsoft VS
Code. Both use the `migoto` language. Open your mod folder, or better, the whole XXMI game folder (the one
with `d3dx.ini`), so library references resolve against your installed package.

### Kate

See [clients/kate/README.md](clients/kate/README.md): lupomikti's `Migoto` syntax for
highlighting, plus a one-time LSP client setting.

### Claude Code

```bash
claude plugin marketplace add xxmi-lang/xxmi-lang
claude plugin install xxmi@xxmi-lang
```

(For one session from a clone: `claude --plugin-dir path/to/xxmi-lang/plugin`.)

The plugin starts `xxmi-lsp` (it must be on your `PATH`) for `.ini` files and adds the
`xxmi-ini-authoring` skill. Enable it per project, in your mod or package folder. Diagnostics
reach the agent after its edits.

### Hosted MCP server

`packages/mcp-server` is a read-only MCP server over Streamable HTTP for agents without a shell
or a local clone: spec lookups with DLL source locations, ZZMI library symbols with their docs,
source search, and `lint_ini`. It indexes `SpectrumQT/XXMI-Libs-Package`,
`leotorrez/ZZMI-Package` and `leotorrez/XXMITools` and follows new commits every 15 minutes.
**No public instance exists yet**; deploying it to Railway is described in
[docs/06-mcp-server.md](docs/06-mcp-server.md). Once you have a URL:

```bash
claude mcp add --transport http xxmi https://<your-domain>/mcp
```

Other clients take the same URL in their HTTP MCP config. The Claude Code plugin reads the URL
from `XXMI_MCP_URL`. [AGENTS.md](AGENTS.md) lists the tools for agents.

### .NET

[![XxmiLang on nuget.org](https://img.shields.io/nuget/v/XxmiLang?label=XxmiLang&logo=nuget)](https://www.nuget.org/packages/XxmiLang)
[![XxmiLang.Core on nuget.org](https://img.shields.io/nuget/v/XxmiLang.Core?label=XxmiLang.Core&logo=nuget)](https://www.nuget.org/packages/XxmiLang.Core)

```bash
dotnet add package XxmiLang
dotnet add package XxmiLang.Native.win-x64      # or linux-x64, osx-arm64, …: the platform(s) you ship
```

A typed C# client over the same binary: `XxmiLinter.LintAsync(paths)` and a long-lived
`XxmiLanguageService` (diagnostics, hover, definition, references, completion, outline, rename,
quick fixes). See [docs/10-dotnet.md](docs/10-dotnet.md).

### Command line

```bash
xxmi lint path/to/mod                  # text output; exit 1 if there are errors
xxmi lint path/to/mod --format json    # schema: packages/cli/schema/lint-output.v1.json
xxmi lint --help                       # every rule, with its default severity
xxmi query command store               # docs, args and DLL source of a command
xxmi query symbol 'CommandList\ZZMI\SetTextures'   # a library symbol: location, docs, usage
xxmi query library                     # libraries (namespaces) with symbol counts
xxmi lsp --stdio                       # the language server (same as xxmi-lsp)
```

A mod linted on its own still resolves `CommandList\ZZMI\…`: the release bundles the ZZMI
library symbols. Inside an XXMI install (a folder with `d3dx.ini` above the mod), the installed
package is used instead.

Configure rules per project in `xxmi.toml`:

```toml
[rules]
XM204 = "off"       # unused variables
XM102 = "error"     # unknown keys
```

and silence one line with `; xxmi-disable-next-line XM205`.

## Development

Node 22.18+ and pnpm. See [CLAUDE.md](CLAUDE.md) for the commands and the rules this repo
follows, and [docs/](docs/) for the design.

```bash
pnpm install && pnpm build && pnpm test
pnpm xxmi lint path/to/mod             # CLI from source
docker build -f packages/mcp-server/Dockerfile -t xxmi-mcp .   # the MCP server image
pnpm build:binaries --target host      # single-file binaries (needs Bun)
```

## Credits

- **lupomikti**, for [tree-sitter-migoto](https://github.com/lupomikti/tree-sitter-migoto),
  which our [fork](https://github.com/xxmi-lang/tree-sitter-migoto) builds on, and for the
  VS Code, Kate and other highlighting this project relies on.
- **SpectrumQT** and the **XXMI** project, for the
  [XXMI-Libs-Package](https://github.com/SpectrumQT/XXMI-Libs-Package) DLL source the language
  spec is extracted from.
- **leotorrez** and the **AGMG** community, for
  [ZZMI-Package](https://github.com/leotorrez/ZZMI-Package) and its libraries.
- **bo3b**, **DarkStarSword** and the [3DMigoto](https://github.com/bo3b/3Dmigoto) authors.

## License

GPL-3.0-or-later. The grammar build from tree-sitter-migoto is MIT
(`packages/core/grammar/LICENSE.tree-sitter-migoto`).
