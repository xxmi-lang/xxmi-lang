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
   [`lspclient-settings.json`](lspclient-settings.json). If `xxmi-lsp` isn't on your `PATH`,
   put the full path in `command`, e.g. `["C:\\Tools\\xxmi-lsp.exe", "--stdio"]`.

`rootIndicationFileNames` makes Kate treat the folder with `d3dx.ini` (your XXMI install) or an
`xxmi.toml` as the project root, so references into the ZZMI libraries resolve.
