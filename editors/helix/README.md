# Helix setup

Helix's built-in LSP client runs `xxmi-lsp`. No Helix grammar for Migoto exists, so `.ini`
files get no syntax highlighting; the language entry only starts the server.

1. **Server.** Install `xxmi-lsp` and put it on your `PATH`: copy-paste commands for each
   system are in [INSTALL.md](../../INSTALL.md).
2. **Language config.** Copy [`languages.toml`](languages.toml) into your Helix config folder:
   - Linux and macOS: `~/.config/helix/languages.toml`
   - Windows: `%APPDATA%\helix\languages.toml`

   Copying replaces an existing `languages.toml`. If you already have one, add the
   `[language-server.xxmi]` and `[[language]]` tables from this file to it instead.

3. **Check.** Run `hx --health migoto`: it should list `xxmi-lsp` as the configured language
   server and find it on your PATH. Open a mod `.ini` and the server starts for it.

The `roots` entry makes Helix treat the folder with `d3dx.ini` (your XXMI install) or an
`xxmi.toml` as the project root, so references into the ZZMI libraries resolve.
