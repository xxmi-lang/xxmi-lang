# Editor config

Every release zip (`xxmi-<version>-<system>.zip`) has an `editors/` folder with the editor config
files and `manifest.json`. The manifest tells an installer which file goes where. Setup steps for
people are in [INSTALL.md](../INSTALL.md#set-up-your-editor).

| Path            | What it is                                                                         |
| --------------- | ---------------------------------------------------------------------------------- |
| `manifest.json` | The install manifest (below). The zipped copy also has `version`.                  |
| `neovim/`       | Neovim 0.11+ config: `lsp/xxmi.lua` and `plugin/xxmi.lua`.                         |
| `kate/`         | Kate LSP settings (`lspclient-settings.json`) and setup guide (`README.md`).       |
| `helix/`        | Helix config snippet (`languages.toml`) to merge by hand, and setup guide.         |
| `sublime/`      | Sublime LSP settings snippet (`LSP.sublime-settings`) to merge by hand, and guide. |

This README isn't in the zip. The zip carries the manifest, the files it lists, and the `helix/` and
`sublime/` folders: those are snippets the user merges by hand, so the manifest doesn't list them.

## Manifest

`manifest.json` is schema version 1.

| Field             | Meaning                                                                                 |
| ----------------- | --------------------------------------------------------------------------------------- |
| `manifestVersion` | Schema version. `1`.                                                                    |
| `version`         | Release version. Only in the zipped copy; the repo copy has none.                       |
| `roots`           | Root key to base folder, per OS. OS keys: `linux`, `linux-flatpak`, `macos`, `windows`. |
| `files`           | The files to install (table below).                                                     |

Paths in `roots` use `~` (home folder) and `%LOCALAPPDATA%`.

Each entry in `files`:

| Field              | Meaning                                                                                                                                                |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `editor`           | Editor name, `neovim` or `kate`.                                                                                                                       |
| `root`             | Key into `roots`.                                                                                                                                      |
| `target`           | Path relative to the root, with `/` as the separator.                                                                                                  |
| `source`           | Path inside the zip. Copied to `target`.                                                                                                               |
| `url` and `sha256` | Download `url`. The file's SHA-256 (hex) must equal `sha256`.                                                                                          |
| `mode`             | `copy` (default) or `merge-json`: merge this file's `servers` entry into the JSON file at `target`. The manifest doesn't say how to resolve conflicts. |

An entry has either `source` or `url`. Kate's syntax files are downloaded from lupomikti's
repository at a pinned commit (they are GPL-3.0), so we don't ship a competing grammar.

Manifest v1 is additive-only: a later v1 may add optional fields. Consumers must ignore fields
they don't know.

## How an installer applies it

For each entry in `files`:

1. Pick the OS key for the entry's root: `linux`, `macos` or `windows`. Use `linux-flatpak`
   for a Flatpak Kate. Skip the entry if the root has no key for that OS.
2. Expand `~` and `%LOCALAPPDATA%` and join the root with `target`.
3. Apply the entry:
   - `copy`: write the `source` file from the zip to the target path.
   - `merge-json`: merge the `servers` entry into the existing file. On Linux and macOS, replace
     `xxmi-lsp` in the merged `command` with the absolute path of the installed binary: Kate doesn't
     expand `~`, and a desktop-launched Kate may lack the shell's PATH. On Windows the bare name works
     if `xxmi-lsp` is on PATH.
   - `url`: download the file and check its SHA-256 against `sha256` before writing. On a
     mismatch, write nothing for this entry.

The Kate settings use the plain command name `xxmi-lsp`. On Linux and macOS, Kate needs the full
path, which the installer doesn't fill in; see [kate/README.md](kate/README.md).

The Windows and macOS Kate paths come from Qt's standard locations. They aren't tested on those
systems.

## Manual install

### Neovim

Neovim 0.11 or newer. Copy the two files from `neovim/` into your config folder:

- Linux and macOS: `~/.config/nvim/lsp/xxmi.lua` and `~/.config/nvim/plugin/xxmi.lua`.
- Windows: `%LOCALAPPDATA%\nvim\lsp\xxmi.lua` and `%LOCALAPPDATA%\nvim\plugin\xxmi.lua`.

The config starts plain `xxmi-lsp`, so the binary must be on your PATH.

### Kate

See [kate/README.md](kate/README.md).

### Helix

See [helix/README.md](helix/README.md).

### Sublime Text

See [sublime/README.md](sublime/README.md).
