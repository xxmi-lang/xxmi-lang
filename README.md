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
  edit.

The language rules aren't hand-written: they're extracted from the XXMI DLL source
(`spec/generated/`), and anything XXMI added over vanilla 3DMigoto is marked as such.

## Install

**Step-by-step for Arch / CachyOS, other Linux distros, Windows and macOS:
[INSTALL.md](INSTALL.md).**

In short, download from the [latest release](https://github.com/xxmi-lang/xxmi-lang/releases/latest):

| File                   | What it is                                                   |
| ---------------------- | ------------------------------------------------------------ |
| `xxmi-<os>-<arch>`     | the CLI (`xxmi lint`, `xxmi index`)                          |
| `xxmi-lsp-<os>-<arch>` | the language server, for Kate, Claude Code and other editors |
| `xxmi-lang.vsix`       | the VS Code extension (server included)                      |

Targets: `windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`, `darwin-x64`,
`darwin-arm64`. The binaries are self-contained (no Node needed). Rename them to `xxmi` /
`xxmi-lsp` (`.exe` on Windows) and put them on your `PATH`; INSTALL.md has copy-paste commands
for each system.

### VS Code

Install `xxmi-lang.vsix` (Extensions view → `…` → Install from VSIX). For highlighting, also
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

### Command line

```bash
xxmi lint path/to/mod                  # text output; exit 1 if there are errors
xxmi lint path/to/mod --format json    # schema: packages/cli/schema/lint-output.v1.json
xxmi lint --help                       # every rule, with its default severity
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
