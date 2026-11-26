# Sublime Text setup

Sublime Text runs `xxmi-lsp` through the **LSP** package. The server is selected by the
`source.migoto` scope, which comes from lupomikti's `Migoto` syntax, so that syntax is required.
The installer doesn't touch this file: merge it into your own LSP settings by hand. This config
is untested in Sublime Text.

1. **Server.** Install `xxmi-lsp` and put it on your `PATH`: copy-paste commands for each
   system are in [INSTALL.md](../../INSTALL.md).
2. **Highlighting.** Copy the `sublime-text-pkg` folder from
   [lupomikti/3dmigoto-ini-extension](https://github.com/lupomikti/3dmigoto-ini-extension/tree/main/sublime-text-pkg)
   into your `Packages` folder (Preferences → Browse Packages…). Its `Migoto.sublime-syntax`
   opens `.ini`, `.3dm` and `.migoto` files.
3. **LSP.** Install the **LSP** package with Package Control.
4. **Server config.** Run **Preferences: LSP Settings** and merge the `xxmi` entry from
   [`LSP.sublime-settings`](LSP.sublime-settings) into the `"clients"` object of
   `Packages/User/LSP.sublime-settings`. In newer LSP versions, run
   **Preferences: LSP Server Configurations** instead and put the `xxmi` entry's contents, without
   the `"clients"` wrapper, into `Packages/User/LanguageServers.sublime-settings` under
   `"xxmi"`. `Packages/User` is:
   - macOS: `~/Library/Application Support/Sublime Text/Packages/User/`
   - Linux: `~/.config/sublime-text/Packages/User/`
   - Windows: `%AppData%\Sublime Text\Packages\User\`

   Restart Sublime Text, or run **LSP: Restart Servers** from the command palette.

On macOS and Linux, a Sublime Text started from the Dock or the desktop menu may not see the
PATH your terminal has. If the server doesn't start, replace `"xxmi-lsp"` in `command` with the
full path (for example `/Users/YOU/.local/bin/xxmi-lsp`; `echo ~/.local/bin/xxmi-lsp` prints
yours). Sublime doesn't expand `~`, so write the path out in full.
