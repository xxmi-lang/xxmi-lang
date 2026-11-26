# Helix setup

Helix's built-in LSP client runs `xxmi-lsp`. The config adds the server to Helix's built-in `ini`
language, so Migoto files keep Helix's INI highlighting; there is no Migoto grammar for Helix.
The installer doesn't touch this file: merge it into your own Helix config by hand.

1. **Server.** Install `xxmi-lsp` and put it on your `PATH`: copy-paste commands for each
   system are in [INSTALL.md](../../INSTALL.md).
2. **Language config.** Put the contents of [`languages.toml`](languages.toml) in
   `.helix/languages.toml` inside your mods folder. Helix merges project, user and built-in
   configs, so only `.ini` files under that folder use the server.

   To use it everywhere instead, append the same tables to your user file
   (`~/.config/helix/languages.toml` on Linux and macOS, `%AppData%\helix\languages.toml` on
   Windows). Every `.ini` file then starts the server.

3. **Check.** Run `hx --health ini` from the mods folder: it should list `xxmi-lsp` as the
   configured language server and find it on your PATH.

The `roots` entry makes Helix treat the folder with `d3dx.ini` (your XXMI install) or an
`xxmi.toml` as the project root, so references into the ZZMI libraries resolve.
