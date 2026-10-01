---
name: xxmi-ini-authoring
description: Use when reading, writing or fixing XXMI / 3DMigoto mod .ini files (ZZMI, GIMI, SRMI, WWMI, HIMI): sections like [TextureOverride…], [CommandList…], [Resource…], run =, ref/copy, $variables, namespaces. Covers checking that the xxmi tooling is installed, acting on its diagnostics, and the conventions to follow.
---

# Authoring XXMI mod .ini files

The `xxmi` tools know the language from the XXMI DLL source. Trust their diagnostics and hover
docs over your memory of 3DMigoto: XXMI added features (`store`, pools, `->Region(…)`, bitwise
operators, `locked` globals) and dropped others (most stereo settings).

## 1. Check the tooling

Run `xxmi-lsp --version` and `xxmi --version` (or `xxmi lint --help`).

- If `xxmi-lsp` is missing, the plugin can't push diagnostics after your edits. Tell the user
  to download `xxmi-lsp` and `xxmi` for their platform from
  https://github.com/xxmi-lang/xxmi-lang/releases and put them on PATH, then restart Claude Code.
- Without the LSP you can still lint by hand: `xxmi lint <file or folder>`.

## 2. After every edit, fix what the tools report

Diagnostics arrive after each edit (from the LSP) or from `xxmi lint`. Before moving on, fix
every error:

- `XM0xx` syntax: the line doesn't parse. Fix the line itself.
- `XM1xx` structure: unknown section or key, bare line, duplicate key, bad enum value,
  unbalanced `if`/`endif`, wrong argument count.
- `XM2xx` references: unresolved `run =`/`ref`/`Resource\N\X`, unknown variable or namespace,
  missing include or `filename =` file.

When a diagnostic says "Did you mean …", that's usually the fix. Don't silence a rule with
`; xxmi-disable-next-line XMnnn` unless the user agrees, and then add a comment saying why.

Use `xxmi lint --format json <path>` when you need the diagnostics as data; exit code 1 means
errors remain.

## 3. Conventions

- **Namespaces.** Start every new file with `namespace = <Name>`. From another file, refer to
  its sections as `CommandList\<Name>\Foo` and its globals as `$\<Name>\var`; inside the same
  namespace, `CommandListFoo` and `$var` are enough.
- **Libraries (ZZMI).** Call library code instead of copying it, e.g.
  `run = CommandList\ZZMI\SetTextures`. Hover a library command list to read its documented
  inputs, and set those before the `run`. Never assign to a library's `locked` globals.
- **Variables.** Declare globals in `[Constants]` (`global $x = 0`, `global persist $x` for
  values the user toggles); use `local $tmp` for temporaries inside a command list.
- **Docs.** Put a `;` comment block directly above every new `[CommandList…]` saying what it
  does and what it expects to be set.
- **Paths.** `filename =` and `include =` are relative to the .ini's folder; `\` and `/` both
  work, and lookup is case-insensitive like Windows.
