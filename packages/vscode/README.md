# XXMI Language Tools

Language support for XXMI / 3DMigoto mod `.ini` files (ZZMI first; the other XXMI games
work through the same mechanism):

- **Diagnostics** as you type: unresolved `run =` targets and resources, unknown keys and
  sections, unbalanced `if`/`endif`, missing include and `filename =` files, and more. Messages
  quote the DLL's own warnings, so editor and game log agree.
- **Go to definition**, **find references** and **rename** for command lists, resources,
  custom shaders and variables, across files and into the ZZMI libraries.
- **Hover** docs for keys, commands and operators, taken from the XXMI DLL source.
- **Completion** for section names, keys, commands, variables, references and file paths.
- **Quick fixes** for "did you mean" suggestions.

Syntax highlighting comes from lupomikti's
[3DMigoto INI](https://github.com/lupomikti/migoto-vscode) extension. Install it alongside this
one; both use the `migoto` language.

## Settings

- `xxmi.rules`: rule severities, e.g. `{ "XM204": "off" }`. A project's `xxmi.toml` works too;
  this setting wins.
- `xxmi.lsp.path`: use an `xxmi-lsp` binary instead of the bundled server.

Rule reference, CLI and other editors: https://github.com/xxmi-lang/xxmi-lang

## Credits

Built on lupomikti's tree-sitter-migoto grammar and editor work, and on the XXMI and AGMG
projects (SpectrumQT's XXMI-Libs-Package, leotorrez's ZZMI-Package).
