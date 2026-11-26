# Sublime Text setup

Sublime Text runs `xxmi-lsp` through the **LSP** package. Highlighting comes from lupomikti's
`Migoto` syntax (scope `source.migoto`), which the LSP config selects on.

1. **Server.** Install `xxmi-lsp` and put it on your `PATH`: copy-paste commands for each
   system are in [INSTALL.md](../../INSTALL.md).
2. **Highlighting.** Copy the `sublime-text-pkg` folder from
   [lupomikti/3dmigoto-ini-extension](https://github.com/lupomikti/3dmigoto-ini-extension/tree/main/sublime-text-pkg)
   into your `Packages` folder (Preferences → Browse Packages…). Its `Migoto.sublime-syntax`
   opens `.ini`, `.3dm` and `.migoto` files.
3. **LSP.** Install the **LSP** package with Package Control.
4. **Server config.** Copy [`LSP-xxmi.sublime-settings`](LSP-xxmi.sublime-settings) into your
   `User` package folder:
   - macOS: `~/Library/Application Support/Sublime Text/Packages/User/`
   - Linux: `~/.config/sublime-text/Packages/User/`
   - Windows: `%APPDATA%\Sublime Text\Packages\User\`

   Restart Sublime Text, or run **LSP: Restart Servers** from the command palette.

On macOS and Linux, a Sublime Text started from the Dock or the desktop menu may not see the
PATH your terminal has. If the server doesn't start, replace `"xxmi-lsp"` in `command` with the
full path (for example `/Users/YOU/.local/bin/xxmi-lsp`; `echo ~/.local/bin/xxmi-lsp` prints
yours). Sublime doesn't expand `~`, so write the path out in full.
