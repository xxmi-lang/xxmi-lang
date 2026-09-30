# 08 — Editor integration

Priority: **VS Code** and **Kate**. Helix, Neovim, Zed and Sublime work later through their generic LSP support.

## Relationship to lupomikti's work

`lupomikti/3dmigoto-ini-extension` provides grammars for VS Code (`migoto-vscode`, language id **`migoto`**, scope `source.migoto`), Kate (`kate-plugin/syntaxes/migoto.xml`, language name **`Migoto`**, extensions `*.ini;*.3dm;*.migoto`), Sublime, Notepad++, Zed and Helix, plus `tree-sitter-migoto`. The author plans an LSP "one day", but has limited time.

Our position: **LSP features on top of their grammars, never competing grammars.** Concretely:

- Use their language ids and names everywhere, so the two stack.
- Credit them in the README and the extension description.
- **PRs to lupomikti's repos: Kate only.** We send PRs to `3dmigoto-ini-extension` for `kate-plugin/syntaxes/migoto.xml` (see "Kate syntax updates" below), since updating it is where their time is shortest. Everything else stays in our repos: the tree-sitter grammar lives in our fork (`01-language-model.md` §2), and VS Code gets our own separate extension.

## VS Code (`packages/vscode`)

A thin client, `vscode-languageclient/node`:

- Activates on language `migoto`. Adds the `xxmi.*` settings (`xxmi.profile`, `xxmi.packagePath`, `xxmi.rules`, `xxmi.lsp.path` to override the bundled server).
- Bundles `@xxmi-lang/lsp` (Node) so no separate install is needed.
- If lupomikti's extension isn't installed, show a one-time prompt linking to it (highlighting comes from there). Until it's on the Marketplace, link to their `.vsix` release.
- Commands: `XXMI: Migrate mod…`, `XXMI: Normalize mod…`, `XXMI: Show detected format era`, `XXMI: Restart server`.
- Semantic tokens: declare `semanticTokenScopes` mapping our token types to the TextMate scopes their grammar already uses (read `source.migoto` in their tmLanguage and pick the matching scopes), so themes color resolved symbols the same way as plain highlighting.
- Publish to both VS Code Marketplace and Open VSX (for VSCodium users).

## Kate

No plugin code needed. Kate's built-in **LSP Client** plugin runs any stdio server.

1. Install lupomikti's `migoto.xml` (their README lists the directories; on Linux `~/.local/share/org.kde.syntax-highlighting/syntax/`, on Flatpak under `~/.var/app/org.kde.kate/data/…`). This gives the `Migoto` highlighting mode.
2. Add to Kate's LSP client user config (Settings → LSP Client → User Server Settings), shipped as `clients/kate/lspclient-settings.json`:

```json
{
  "servers": {
    "migoto": {
      "command": ["xxmi-lsp", "--stdio"],
      "highlightingModeRegex": "^Migoto$"
    }
  }
}
```

3. Turn on semantic highlighting in the LSP Client settings if it's off.

Verify the exact key names against current Kate docs when implementing; the shape above follows Kate's documented `servers` / `command` / `highlightingModeRegex` config. Also verify that `.ini` files in mod folders open in `Migoto` mode rather than generic `INI Files`. If they don't, document a per-folder `.kateconfig` or mode override.

Test on Linux (KDE; both the distro Kate package and the Flatpak) and on Windows (Kate from the KDE installer or Microsoft Store). On Windows the syntax directory is `%USERPROFILE%\AppData\Local\org.kde.syntax-highlighting\syntax`, and `command` should be `["xxmi-lsp.exe", "--stdio"]` or a full path if it isn't on PATH. Kate on macOS works the same way but is lower priority.

### Kate syntax updates (PRs to lupomikti)

Their `migoto.xml` needs to keep up with the XXMI dialect (the same constructs as the tree-sitter gaps: bitwise and shift operators, `store`, pools and `[*]` wildcards, `local`, `->` member access, `Region(...)`, plus anything new the spec extractor finds).

- Source of the keyword and operator lists: `spec/generated/*.json`. Add a small script (`clients/kate/scripts/diff-kate-keywords.ts`) that compares their XML's keyword lists against the spec and prints what's missing. It's reused for every future update.
- Follow their repo's conventions: edit `kate-plugin/syntaxes/migoto.xml` (it's hand-written XML, not generated from their TOML), bump its `version` attribute, and test in Kate with their README's install steps plus a sample file covering every new construct (include that sample in the PR).
- One PR per logical change (e.g. "XXMI operators", "Pool syntax"), small and easy to review, with before/after screenshots.
- Until a PR is merged, `clients/kate/README.md` tells users where to get the patched XML (the PR branch), so they aren't blocked on review.

## HLSL syntax for Kate (`clients/kate/hlsl.xml`)

Kate currently falls back to the wrong highlighter for HLSL files. Deliverable: a KSyntaxHighlighting definition.

- Extensions: `*.hlsl;*.hlsli;*.fx;*.fxh`, plus 3DMigoto replacement shaders `*_replace.txt` if that doesn't clash with other definitions. Use a low priority, or document manual mode selection if it does.
- Cover: SM5 keywords, types (`float4x4`, `Texture2D<float4>`, `RWStructuredBuffer<T>`, `SamplerState`, `cbuffer`), semantics (`: SV_Position`, `: TEXCOORD0`), `register(t0)`, intrinsics list, preprocessor, attributes (`[numthreads(…)]`, `[unroll]`). 3DMigoto decompiled shaders use generated names like `r0.xyzw` and `cb0[12]`; make swizzles readable.
- Base it on KSyntaxHighlighting's existing C-family definitions to inherit comment and number handling.
- Test with every `.hlsl`/`.hlsli` in ZZMI-Package (`Core/ZZMI/Shaders`, `Libraries/TTLib/Shaders`, `HP bar/*.hlsl`).
- Once it's solid, offer it to lupomikti's `kate-plugin/syntaxes/` (they already host a helper syntax there, `pcre2.xml`), and/or propose it to KDE's syntax-highlighting repository so everyone gets it without manual install.

Out of scope: HLSL diagnostics. If needed later, evaluate existing HLSL language servers, but note that 3DMigoto compiles with fxc (SM5, `d3dcompiler_47`), and DXC-based validators will report false errors.
