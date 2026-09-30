# 01 — Language model

This doc covers how we know what the language is (the spec), how we parse it (tree-sitter-migoto), and what we build from the parse tree (symbols, namespaces, the workspace index, the library catalog).

## 1. Spec extraction

### Why extract

The DLL defines the language, and XXMI keeps extending it: `store`, pools, bitwise operators, `local`, `->` member access, `Region(...)`. A hand-written keyword list will drift. `packages/spec-extractor` reads the C++ source and writes `spec/generated/*.json`. A CI job reruns it against the latest XXMI-Libs-Package `master` and opens a PR when the output changes.

### Sources

Line numbers are from `master` as of 2026-09-29 and will move. The extractor must locate these by symbol name, not by line.

| What | Where in the DLL | Notes |
|---|---|---|
| Command-list section prefixes | `IniHandler.cpp`, `static Section CommandListSections[]` | `{name, prefix}` pairs: TextureOverride, CommandList, Constants, Present, ShaderOverride, CustomShader, ShaderRegex, BuiltInCommandList, BuiltInCustomShader, Clear* |
| Regular section prefixes | `IniHandler.cpp`, `static Section RegularSections[]` | Pool, Resource, Key, Include, Preset, Hunting, Logging, System, Input, Device, Rendering, Loader, Profile, … |
| Lines allowed without `=` | `IniHandler.cpp`, `AllowLinesWithoutEquals[]` | Plus all command-list sections (for if/else/endif) |
| Duplicate-key whitelist | `IniHandler.cpp`, `whitelisted_duplicate_key()` | `key` and `back` in `[Key*]`; anything in `[Include]` |
| Per-section keys | `IniHandler.cpp`, the `Parse*Section`/`Get*` functions that read keys (e.g. resource `type`, `format`, `filename`, `data`, `stride`, `array`; key `key`, `type`, `condition`, …) | Harder to extract mechanically. Extract calls like `GetIniString(section, L"format", …)` / `GetIniInt`/`GetIniEnum`… with a regex over call sites, grouped by the enclosing parse function |
| General commands | `CommandList.cpp`, `ParseCommandListGeneralCommands()` | `checktextureoverride`, `handling`, `store`, `run`, draw commands, … Collect `!wcscmp(key, L"…")` comparisons |
| Draw commands | `CommandList.cpp`, `ParseDrawCommand()` / `ParseDrawCommandArgs()` | Name and arg count |
| Flow control | `CommandList.cpp`, `ParseIfCommand` / `ParseElseIfCommand` / `ParseElseCommand` / `ParseEndIfCommand` | |
| Operators | `CommandList.cpp`, `static const wchar_t *operator_tokens[]` | Includes `<<`, `>>`, `**`, `//`, `&&`, `\|\|`, … Also read the precedence table the tokenizer uses |
| `local` declarations | `CommandList.cpp`, the `line.compare(0, 5, L"local")` branch | |
| Resource member access | `CommandList.cpp`, the `->` handling (e.g. `ResourceFoo->HashRegion($offset, $size)`, `vb0->Offset`, `->Region(...)`) | |
| Pool semantics | `IniHandler.cpp`, Pool section parsing (around the `lower_bound(L"Pool")` loop) | Index types, persistence ("ring" only), wildcard `[*]` |
| Enums (formats, types, usage flags, shader stages, slots) | DirectX11 headers and the enum tables used by `GetIniEnum*` | DXGI formats may be easier to take from a static list |

### Output schema (sketch)

```jsonc
// spec/generated/sections.json
{
  "dllCommit": "…", "extractedAt": "…",
  "sections": [
    {
      "name": "Resource", "prefix": true, "kind": "regular",
      "keys": [
        { "name": "type", "valueType": "enum:ResourceType", "source": "IniHandler.cpp:ParseResourceSections" },
        { "name": "format", "valueType": "enum:DXGI_FORMAT", "source": "…" },
        { "name": "filename", "valueType": "path", "source": "…" }
      ]
    },
    { "name": "CommandList", "prefix": true, "kind": "commandlist", "allowsBareLines": true }
  ]
}
// spec/generated/commands.json
{ "commands": [ { "name": "store", "args": ["variable", "resource", "index"], "contexts": ["commandlist"], "source": "CommandList.cpp:ParseCommandListGeneralCommands", "xxmi": true } ] }
```

`spec/overlay/*.toml` adds what extraction can't provide: human docs, examples, better arg types, deprecation notes, and "since XXMI version". The loader deep-merges overlay over generated. Overlay entries for names that don't exist in generated are an error, unless marked `source = "manual"`.

### Dialects

`meta.json` records the DLL commit. Profiles may pin a minimum DLL version. Anything from the XXMI fork and not in vanilla 3DMigoto gets `"xxmi": true`, so hover can say "XXMI-only".

## 2. Parser: tree-sitter-migoto

Use **our fork** of lupomikti's grammar (`lupomikti/tree-sitter-migoto`, MIT, v0.9.3 as of 2026-09-28) through `web-tree-sitter`. Ship the `.wasm` inside `@xxmi-lang/core`.

Fork policy:

- Keep the fork as a normal GitHub fork (so the relationship is visible), under the project's org. Keep MIT and lupomikti's copyright notice; add ours for our changes.
- We don't open PRs to upstream. The fork should stay easy for lupomikti to pull from if they choose: small commits, each with a `test/corpus/` case, no reformatting of unrelated grammar code.
- Periodically merge upstream `main` into the fork (a monthly CI job opens a PR in the fork when upstream has new commits). Resolve conflicts in favor of upstream's structure, then re-apply our fixes.
- Keep node names compatible with upstream where possible, so their highlight and other queries keep working on our fork.
- Consume the fork in `@xxmi-lang/core` via a pinned git dependency or a published package (e.g. `@xxmi-lang/tree-sitter-migoto`); pin to a commit or version.
- **Current setup (M0):** the fork's WASM build is vendored at `packages/core/grammar/tree-sitter-migoto.wasm`, with `SOURCE.json` recording the fork commit, upstream version, tree-sitter CLI version and SHA-256 (a unit test checks the hash). Rebuild it from a clean fork checkout with `XXMI_GRAMMAR_SRC=<fork> pnpm grammar:update`. `web-tree-sitter` is pinned to the same version as the CLI that built the WASM. The fork's own CI (`xxmi-ci.yml`) runs `tree-sitter test --wasm` and checks that `src/` is regenerated; `upstream-sync.yml` is the monthly merge job.

### Known dialect gaps (measured 2026-09-30, fixed in M0)

Parsing all 19 `.ini` files in ZZMI-Package with upstream v0.9.3 gave 15 clean and 4 with ERROR nodes. The original notes (the "Suspected" column) listed several candidate constructs per file; bisecting narrowed each file to one cause, and for `Matches.ini` the cause was something else entirely. All four are fixed in the fork (`XXMI-CHANGES.md` entries 1–4, each with a `test/corpus/xxmi.txt` case), and all 19 files now parse without ERROR or MISSING nodes.

| File | Suspected | Actual cause | DLL source | Fork fix |
|---|---|---|---|---|
| `Core/ZZMI/Libraries/HP bar/hp.ini` | `store = $x, Resource.UAV, 7`, `>>`, `&` | The external scanner read `Resource.UAV,` (with the comma) as the resource name. `>>` and `&` were fine | `CommandArgumentReader::GetTokenInternal` stops tokens at `,` | `,` ends a custom resource identifier (`src/scanner.c`) |
| `Core/ZZMI/Libraries/SlotFix/Matches.ini` | multi-segment `namespace` | `_resource_format` was `/DXGI_FORMAT_.+/`, eating `)`/`\|\|` to the end of the line. The namespace was fine | `tokenise()` → `FindIdentifierTokenEnd` (`[a-z0-9_]`) | `DXGI_FORMAT_[a-z0-9_]+`, same for `D3D11_BIND_` |
| `Core/ZZMI/Libraries/SlotFix/SlotFix.ini` | `$Poolt[*] = …` | `Poolt = null`: a pool without an index. The wildcard was fine | `ResourceCopyTarget::ParseTargetPool` (no index → whole pool) | bare pool is a resource operand (also fixes `PoolFoo->Size`) |
| `Core/ZZMI/Libraries/TTLib/UIPlacement.ini` | `->Offset`, `->Region`, `ref PoolX[$slot]` | only `copy vb0->Region(...)`: a member suffix on a copy source | `ResourceCopyTarget::ParseTarget` → `ParseTargetMember` | `resource_usage_expression` accepts `property_access_expression` |

Minimal license-clean reproductions live in `fixtures/parser/xxmi-dialect/`.

### Error recovery

tree-sitter's recovery is heuristic. Most errors stay local (an unterminated `if` ends at the next section header), but some swallow the rest of the file: an unclosed `(` in `$y = ($x + 1` becomes one ERROR node that also covers every later section (`fixtures/parser/errors/unclosed-paren.ini`, and an `it.fails` test in `packages/core/test/parser.test.ts`). Lowering (M2) must handle this so errors don't hide later sections, for example by reparsing from the next section header when an ERROR node spans one.

### Parser contract for core

- `parse(text) → Tree` (incremental: `tree.edit` plus reparse on LSP `didChange`).
- Implemented in M0 (`packages/core/src/parser/`): `initParser`, `createParser` (a parser of your own, for incremental use), `parseText` and `parseFile`, which return the tree plus its ERROR/MISSING nodes as `SyntaxIssue`s (outermost ERROR only, plus every MISSING). Positions are 0-based lines and UTF-16 columns, the LSP default. `parseFile` strips a UTF-8 BOM and decodes invalid UTF-8 lossily, reporting encoding `unknown` (the future XM002). Trees own WASM memory: call `tree.delete()`.
- A CST-to-model lowering pass (`lower.ts`) builds the typed model below and never throws. ERROR and MISSING nodes become `XM001` diagnostics (see `02-diagnostics.md`), and lowering continues with the next section.
- Keep a thin fallback line scanner: section headers plus `key = value` split. If the tree-sitter WASM fails to load, the CLI still gives basic lint.

## 3. Semantic model

```ts
interface IniFile { uri; namespace: Namespace; sections: Section[]; encoding: 'utf8'|'utf8bom'|'gbk'|…; }
interface Section { name: string; prefix: SectionKind; localName: string; range; entries: Entry[]; doc?: DocComment; }
type Entry = KeyValue | Command | IfBlock | BareLine | Comment;
interface Symbol { kind: 'section'|'variable'|'pool'|'resource'|'commandlist'|'customshader'|'key'|'preset';
                   qualifiedName: string /* \ns\LocalName */; declaration: Location; scope: 'global'|'local'|'section'; flags: {persist?, global?, locked?}; doc?: DocComment; }
interface Reference { target: string /* as written */; resolved?: Symbol; location: Location; }
```

### Namespace resolution rules

Confirm each rule against `_get_section_namespace`, `_get_section_path` and `_get_namespaced_section_path` in `IniHandler.cpp`. Test each rule with fixtures.

1. A file's namespace is the value of `namespace = …` if present. Several files may share one namespace (ZZMI uses `ZZMIv1` in `main.ini` and `ZZMI` in `SlotFix/Aliases.ini`).
2. Without a declaration, sections are namespaced by the file's path, as the DLL does for included and recursively included files.
3. A section `[CommandListFoo]` in namespace `N` is referenced from other namespaces as `CommandList\N\Foo`. From the same namespace it can be `CommandListFoo`.
4. Section local names may contain dots (`[CommandList.Set.FromAlias]`) and spaces (`[Include HP bar]`). Treat them as opaque after the prefix.
5. Variables: `$name` resolves in the current namespace; `$\N\name` is cross-namespace (`$\TTL\alpha`, `$\SlotFix\Matches\slot`). `local $x` is scoped to the command list.
6. Resource refs: `ref ResourceX`, `Resource\N\X`, `copy`, and `ref … unless_null`.
7. Lookups are case-insensitive, matching `WStringInsensitiveLess`.

### File paths: always Windows semantics

The DLL runs on Windows (or under Wine/Proton on Linux), so paths inside inis follow Windows rules no matter which OS the tooling runs on:

- `\` and `/` are both separators in `include =`, `filename =` and similar keys. Never pass an ini path straight to Node's `path`; use `packages/core/src/paths.ts`, which normalizes them.
- File lookup is **case-insensitive** even on case-sensitive filesystems (Linux, some macOS setups). Resolve by scanning the directory and matching case-insensitively, and cache the listing. If a Linux user's file only matches with a different case, emit a hint, since it works in game but may confuse other tools.
- Paths may contain spaces and non-ASCII characters (`Libraries\HP bar\`, CN folder names); XXMI supports UTF-8 paths.
- Output paths (diagnostics, CLI, MCP) use `/` when shown to users, but edits written back into inis keep the file's existing separator style.
- Line endings: accept CRLF, LF and mixed; preserve per file (see `05-transforms.md`).

### Doc comments

Most library docs are free-form `;` comments (see `main.ini` "Input/Usage" blocks and TTLib's comments above `global` vars). Two layers:

1. **Heuristic (always on):** the contiguous `;` comment block directly above a section header, or above a `global`/`persist` variable line, becomes its doc. A block starting `; Usage:` at file top becomes the file's module doc.
2. **Structured (opt-in convention to propose to ZZMI maintainers):**
   ```ini
   ;; @desc Sets ZZMI texture slots from the given resources.
   ;; @in   Resource\ZZMI\Diffuse   diffuse texture (required)
   ;; @in   Resource\ZZMI\NormalMap (optional)
   ;; @since ZZMI 2.x
   ;; @example
   ;;   Resource\ZZMI\Diffuse = ref ResourceBodyDiffuse
   ;;   run = CommandList\ZZMI\SetTextures
   [CommandListSetTextures]
   ```
   `;;` stays a plain comment to the DLL, so it costs nothing at runtime.

## 4. Workspace index

- **Roots:** the opened folder, plus the **package root**. The package root comes from `xxmi.toml` (`package = "../../"`) or is auto-detected by walking up from the mod to a folder containing `d3dx.ini` and `Core/<Game>/`. In a typical XXMI install, `ZZMI/Mods/<mod>/` walks up to `ZZMI/`.
- **Includes:** follow `[Include*]` `include`, `include_recursive` and `exclude_recursive` like the DLL does, including the recursive `Mods/` include from `d3dx.ini`. Files are indexed lazily: open files first, then include targets, then the rest in the background.
- **Bundled library snapshot:** each release of `@xxmi-lang/core` embeds a pre-built index of the latest ZZMI package libraries (symbols and docs only). A user with just a mod folder still gets completion and hover for `CommandList\ZZMI\…`. A local package, when found, overrides the snapshot.
- **Invalidation:** file watcher (LSP `workspace/didChangeWatchedFiles`). Re-lower only changed files, then re-resolve references in files that depend on changed symbols.

## 5. Game profiles

`spec/profiles/zzmi.toml` (fill in with maintainers):

```toml
game = "ZZZ"
package_dir_markers = ["d3dx.ini", "Core/ZZMI/main.ini"]
core_namespace = "ZZMIv1"
libraries = ["SlotFix", "TTLib", "HP bar"]     # auto-discovered; this list is only for docs ordering
format_eras = ["position", "blend-v1", "blend-v2"]   # names TBD, see 05-transforms.md
hash_sources = []                                    # TBD: where component hash mappings come from
```
