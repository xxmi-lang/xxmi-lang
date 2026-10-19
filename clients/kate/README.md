# Kate setup

Kate's built-in **LSP Client** plugin runs `xxmi-lsp`; no Kate plugin is needed. Highlighting
comes from lupomikti's `Migoto` syntax definition.

1. **Highlighting.** Install `migoto.xml` from
   [lupomikti/3dmigoto-ini-extension](https://github.com/lupomikti/3dmigoto-ini-extension/tree/main/kate-plugin)
   into your syntax folder:
   - Linux: `~/.local/share/org.kde.syntax-highlighting/syntax/`
   - Linux (Flatpak): `~/.var/app/org.kde.kate/data/org.kde.syntax-highlighting/syntax/`
   - Windows: `%USERPROFILE%\AppData\Local\org.kde.syntax-highlighting\syntax\`

   Restart Kate. `.ini` files should open in the **Migoto** mode (Tools → Mode). If a mod's
   files open as "INI Files" instead, pick Migoto from that menu (with its default settings, Kate keeps the choice for files it has seen before).

2. **Server.** Install `xxmi-lsp` and put it on your `PATH`: copy-paste commands for each
   system are in [INSTALL.md](../../INSTALL.md).
3. **LSP client.** Enable Settings → Configure Kate → Plugins → **LSP Client**, then open
   Settings → Configure Kate → LSP Client → **User Server Settings** and paste
   [`lspclient-settings.json`](lspclient-settings.json). On Linux and macOS, replace
   `"xxmi-lsp"` in `command` with the **full path** (e.g. `/home/YOUR_NAME/.local/bin/xxmi-lsp`;
   `echo ~/.local/bin/xxmi-lsp` prints yours). Kate started from the desktop menu doesn't get
   the PATH your terminal has, so the plain name may not be found, especially with fish (the
   CachyOS default). Kate doesn't expand `~` or `$HOME`, so write the path out in full. On
   Windows, the plain `xxmi-lsp.exe` works once it's on your PATH (see
   [INSTALL.md](../../INSTALL.md#what-putting-it-on-path-does-and-doesnt-do)).

`rootIndicationFileNames` makes Kate treat the folder with `d3dx.ini` (your XXMI install) or an
`xxmi.toml` as the project root, so references into the ZZMI libraries resolve.
