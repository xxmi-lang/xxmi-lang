# 01 — Language model

This doc covers how we know what the language is (the spec), how we parse it (tree-sitter-migoto), and what we build from the parse tree (symbols, namespaces, the workspace index, the library catalog).

## 1. Spec extraction

### Why extract

The DLL defines the language, and XXMI keeps extending it: `store`, pools, bitwise and shift operators, math functions (`sin(…)`, `saturate(…)`, …), `locked` globals, `->` member access, `Region(...)`. (`local` is older: vanilla 3DMigoto already has it.) A hand-written keyword list will drift. `rust/crates/xxmi-spec-extractor` reads the C++ source and writes `spec/generated/*.json`. A CI job reruns it against the latest XXMI-Libs-Package `master` and opens a PR when the output changes.

### Sources

Line numbers are from `master` as of 2026-09-29 and will move. The extractor must locate these by symbol name, not by line.

| What | Where in the DLL | Notes |
|---|---|---|
| Command-list section prefixes | `IniHandler.cpp`, `static Section CommandListSections[]` | `{name, prefix}` pairs: TextureOverride, CommandList, Constants, Present, ShaderOverride, CustomShader, ShaderRegex, BuiltInCommandList, BuiltInCustomShader, Clear* |
| Regular section prefixes | `IniHandler.cpp`, `static Section RegularSections[]` | Pool, Resource, Key, Include, Preset, Hunting, Logging, System, Input, Device, Rendering, Loader, Profile, … |
| Lines allowed without `=` | `IniHandler.cpp`, `AllowLinesWithoutEquals[]` | Plus all command-list sections (for if/else/endif) |
| Duplicate-key whitelist | `IniHandler.cpp`, `whitelisted_duplicate_key()` | `key` and `back` in `[Key*]`; anything in `[Include]` |
| Per-section keys | `IniHandler.cpp`, the `Parse*Section`/`Get*` functions that read keys (e.g. resource `type`, `format`, `filename`, `data`, `stride`, `array`; key `key`, `type`, `condition`, …) | Harder to extract mechanically. Extract calls like `GetIniString(section, L"format", …)` / `GetIniInt`/`GetIniEnum`… with a regex over call sites, grouped by the enclosing parse function |
| General commands | `CommandList.cpp`, `ParseCommandListGeneralCommands()` | `checktextureoverride`, `handling`, `store`, `run`, draw commands, … Collect `!wcscmp(key, L"…")` comparisons; blocks that compare only `val` (`snapshotinitialtextures`, `restoreinitialtextures`) are commands too, marked `"form": "value"` and not offered or accepted as keys |
| Draw commands | `CommandList.cpp`, `ParseDrawCommand()` / `ParseDrawCommandArgs()` | Name and arg count |
| Flow control | `CommandList.cpp`, `ParseIfCommand` / `ParseElseIfCommand` / `ParseElseCommand` / `ParseEndIfCommand` | |
| Operators | `CommandList.cpp`, `static const wchar_t *operator_tokens[]` | Includes `<<`, `>>`, `**`, `//`, `&&`, `\|\|`, … Also read the precedence table the tokenizer uses |
| `local` declarations | `CommandList.cpp`, the `line.compare(0, 5, L"local")` branch | |
| Resource and shader member access | `CommandList.cpp`, the `members[]` tables in `ResourceCopyTarget::ParseTargetMember` (e.g. `ResourceFoo->HashRegion($offset, $size)`, `->Region(...)`) and `ShaderTarget::ParseTargetMember` (`ps->cb_first_slot`, `ps->srv_slot_count`) | Both land in `resourceMembers`, told apart by `target` (`resource` or `shader`); neither function exists in vanilla, so the baseline skips them without a warning |
| Pool semantics | `IniHandler.cpp`, Pool section parsing (around the `lower_bound(L"Pool")` loop) | Index types, persistence ("ring" only), wildcard `[*]` |
| Enums (formats, types, usage flags, shader stages, slots) | DirectX11 headers and the enum tables used by `GetIniEnum*` | DXGI formats may be easier to take from a static list |

### How the extractor finds things (M1)

`xxmi-spec-extractor` reads `DirectX11/*.{cpp,h}` plus the root `util.h`. It strips comments (string-aware, so `L"//"` survives), then finds tables and functions by name:

- **Section tables and key lists:** `{L"Name", bool}` arrays; `wchar_t *XIniKeys[]` whitelists, expanding same-file `#define` macros such as `TEXTURE_OVERRIDE_FUZZY_MATCHES`.
- **Keys and their types:** `GetIni*(section, L"key", …)` call sites, typed by the getter (`GetIniInt` → `int`, `GetIniEnumClass(…, XNames)` → `enum:XNames`, a string passed straight to `ParseFormatString` → `enum:DXGIFormats`). Calls with a literal section name (`GetIniBool(L"Logging", …)`, `RegisterIniKeyBinding(L"Hunting", …)`) are collected from every file. Calls with a variable section are attributed through `SECTION_KEY_SOURCES` in `rust/crates/xxmi-spec-extractor/src/sections.rs`, the one hand-kept map from section to parser function. For command-list sections, only whitelisted keys count as keys; everything else is a command.
- **Commands:** literal comparisons on the key, value or line (`!wcscmp(key, L"…")` is exact, `wcsncmp` / `.compare(0, n, …)` is a prefix match) in the dispatch functions, plus `ParseDrawCommandArgs(…, indirect, nargs, …)` for draw argument counts.
- **Operators:** `operator_tokens[]`, plus precedence from the order of the `transform_operators_recursive(&tree, <group>, …, right_assoc, unary)` calls, and `DEFINE_OPERATOR(name, "pattern", …)` for each group member. Identifier patterns in `unary_operators` are the math functions.
- **Enums:** every `EnumName_t<…> XNames[]` table, plus `DXGIFormats[]` from `util.h`.

A missing anchor makes extraction exit 3 and name what's missing, instead of silently shrinking the spec. `xxmi: true` is not hand-marked. The extractor runs a second, lenient extraction over vanilla 3DMigoto (`bo3b/3Dmigoto`) and flags every entry vanilla lacks, matching names case-insensitively. Where the two DLLs differ in shape, the extractor accepts both: XXMI's `ParseResourceSection` vs. vanilla's `ParseResourceSections`, and `line.compare(0, 5, L"local")` vs. `name.compare(0, 6, L"local ")`. An integration test pins known vanilla features (`run`, `if`, `local`, `global`, Resource `type`/`format`, …) to `xxmi: false`, so an anchor mismatch in the baseline can't quietly mark them XXMI-only.

`gimi: false` marks what the GIMI DLL lacks. Most Genshin players can't use the XXMI DLL (anti-cheat) and run the GIMI DLL, which is XXMI-Libs-Package at tag `v0.1.0` (vanilla GIMI DLL code). With `GIMI_DLL_SRC` set, the extractor runs the same lenient extraction over that checkout and flags every entry it lacks; the field is omitted when the entry is present, and absent entirely when no GIMI source is given. Entries only the `v0.1.0` tree has (removed since, such as the 3D Vision keys) are not added to the spec; the extractor lists them on stderr. Overlay-only (`manual`) entries are never stamped, so they count as present in the GIMI DLL. The extractor does not model the `resource_copy`, `filter` and `pool` command forms as commands, so they carry no flag. Core exposes the flag as `Meta::gimi` (default `true`).

Known limits: keys the DLL builds at runtime (`blend[0]`…`mask[7]`, via `swprintf`) have `valueType: "unknown"`; overlays may supply a type. XXMI no longer reads any `[Stereo]` or `[ConvergenceMap]` keys (vanilla did), so those sections have none.

### Output

`spec/generated/` holds five files, written by `pnpm spec:extract` (`XXMI_DLL_SRC` = XXMI-Libs-Package checkout, `MIGOTO_DLL_SRC` = 3Dmigoto checkout, `GIMI_DLL_SRC` = XXMI-Libs-Package at the `v0.1.0` commit pinned in `meta.json`). The types are in `rust/crates/xxmi-core/src/spec.rs`.

```jsonc
// meta.json: no wall-clock timestamp, so the same commits give byte-identical output
{ "extractorVersion": 1,
  "dll":      { "repository": "https://github.com/SpectrumQT/XXMI-Libs-Package", "commit": "…", "commitDate": "…" },
  "baseline": { "repository": "https://github.com/bo3b/3Dmigoto", "commit": "…", "commitDate": "…" },
  "gimi":     { "repository": "https://github.com/SpectrumQT/XXMI-Libs-Package", "commit": "0a029a74…", "commitDate": "…" } }
// sections.json
{ "sections": [ { "name": "Resource", "prefix": true, "kind": "regular",
    "allowsBareLines": false, "allowsDuplicateKeys": false,
    "keys": [ { "name": "type", "valueType": "enum:CustomResourceTypeNames",
                "source": "IniHandler.cpp:ParseResourceSection", "xxmi": false } ],
    "dynamicKeys": [], "source": "IniHandler.cpp:RegularSections", "xxmi": false } ] }
// commands.json: commands (kind general|draw|flow|declaration|prefix), resourceMembers (resource and shader targets), functions
{ "commands": [ { "name": "store", "kind": "general", "match": "exact", "values": [],
                  "source": "CommandList.cpp:ParseCommandListGeneralCommands", "xxmi": true } ],
  "resourceMembers": [ { "name": "region", "target": "resource", "args": ["unsigned", "unsigned"], … } ],
  "functions": [ { "name": "saturate", … } ] }
// operators.json: tokens, and precedence levels (0 binds tightest)
// enums.json: every EnumName_t table plus DXGIFormats
```

`spec/overlay/*.toml` adds what extraction can't provide: human docs, examples, better value/arg types, deprecation notes, and "since XXMI version". The build (`rust/crates/xxmi-spec-extractor/src/load.rs`) deep-merges the overlays over the generated spec, in file-name order. Tables address entries by name, case-insensitively:

```toml
[commands.store]              # also [members.<name>], [functions.<name>], [enums.<name>]
doc = "…"
example = """…"""
[sections.Resource.keys.type]
doc = "…"
valueType = "enum:CustomResourceTypeNames"   # keys may override valueType; members may set args
[operators."<<"]
doc = "…"
```

An overlay entry for a name the generated spec doesn't have is an error, unless it has `source = "manual"` plus a `note` saying why; it's then added (manual members also need `target`). Overlays can't set `xxmi` or `source` on generated entries, and an unknown field is an error, so typos surface.

### Dialects

`meta.json` records the DLL commit. Profiles may pin a minimum DLL version. Anything from the XXMI fork and not in vanilla 3DMigoto gets `"xxmi": true`, so hover can say "XXMI-only".

DLL 1.3.2 renamed `snapshotoriginaltextures` / `restoreoriginaltextures` to `snapshotinitialtextures` / `restoreinitialtextures`, and replaced `->MatchTextures(...)`, `->GetMatchFormat` and the `match_*` keys of `[Resource]` with `[Filter…]` sections (`match_format`, `match_dhash`, `match_confidence`), `ps-t[FilterX] = ResourceY` slot filters and `->HammingDistance(FilterX)`. The bundled spec is v1.3.3 (stable, 2026-11-19); its only language change is that `run = RestoreInitialTextures` runs in the pre list instead of post (CommandList.cpp:1343). Overlay entries for names a DLL version lacks are skipped with a warning when the MCP server builds that version's spec, so the older names keep their generated entries and lose only their overlay docs; the bundled load stays strict.

## 2. Parser: tree-sitter-migoto

Use **our fork** of lupomikti's grammar (`lupomikti/tree-sitter-migoto`, MIT, v0.9.4 as of 2026-10-19) through the `tree-sitter` Rust crate. The C sources are vendored in `rust/crates/tree-sitter-migoto`.

Fork policy:

- Keep the fork as a normal GitHub fork (so the relationship is visible), under the project's org. Keep MIT and lupomikti's copyright notice; add ours for our changes.
- We don't open PRs to upstream. The fork should stay easy for lupomikti to pull from if they choose: small commits, each with a `test/corpus/` case, no reformatting of unrelated grammar code.
- Periodically merge upstream `main` into the fork (a monthly CI job opens a PR in the fork when upstream has new commits). Resolve conflicts in favor of upstream's structure, then re-apply our fixes.
- Keep node names compatible with upstream where possible, so their highlight and other queries keep working on our fork.
- Vendor the fork's C sources with `pnpm grammar:update`, and pin the commit in `rust/crates/tree-sitter-migoto/SOURCE.md`.
- **DLL 1.3.1 syntax** (the fork keeps parsing these 1.3.1 names for per-version specs; fork branch `xxmi-dll-1.3.1-syntax`, commit `5d78a72`): `run = snapshotoriginaltextures` / `restoreoriginaltextures` parse as a `run_instruction` holding a `run_instruction_key_value` (no section reference), `ps->MatchTextures(ResourceX) = ResourceY` is an `assignment_statement` with a `shader_identifier` target, and member-call arguments accept `hex_integer` (`0x` plus 1-16 hex digits). The model needs no special case: resources inside `MatchTextures(...)` lower as ordinary `custom_resource` references. Fixtures: `fixtures/xxmi-1.3.1/`.
- **DLL 1.3.2 syntax** (fork commit `e0df4ec` on branch `xxmi`): `run = snapshotinitialtextures` / `restoreinitialtextures` parse like the original-texture names. `[Filter…]` is a `setting_section` (its keys are ordinary settings). A `FilterX` reference is a `filter` node (`filter_prefix` plus `section_identifier`) wherever a resource operand goes: filter assignments (`FilterA = ref FilterB`, `= null`), `@FilterX`, `FilterX->format` and arguments such as `->HammingDistance(FilterX)`. In `ps-t[FilterX]` it is the `index_expression` of a `slot_expression`. The scanner leaves `filter_index` a plain setting key. The spec decides which names a DLL version accepts. Fixture: `fixtures/rules/XM001/good.ini`.
- **Names at end of file and single-segment names** (fork commit `2a366ae` on branch `xxmi`, issue #20): a `run = CommandList\Name` value parses with or without a namespace, and the last line of a file needs no trailing newline. The DLL looks the whole value up as a section name (`CommandList.cpp` `FindExplicitCommandListSection`), so an unknown `CommandList\Name` is an XM201 unresolved reference, not a syntax error. Fixtures: `fixtures/rules/XM001/good.ini`, `fixtures/rules/XM201/bad.ini`.
- **Current setup (M0):** the fork's C sources are vendored in `rust/crates/tree-sitter-migoto` (`src/parser.c`, `src/scanner.c`), with `SOURCE.md` recording the fork commit. Refresh them with `XXMI_GRAMMAR_SRC=<fork> pnpm grammar:update`. The fork's own CI (`xxmi-ci.yml`) runs `tree-sitter test --wasm` and checks that `src/` is regenerated; `upstream-sync.yml` is the monthly merge job.

### Known dialect gaps (measured 2026-09-30, fixed in M0)

Parsing all 19 `.ini` files in ZZMI-Package with upstream v0.9.3 gave 15 clean and 4 with ERROR nodes. The original notes (the "Suspected" column) listed several candidate constructs per file; bisecting narrowed each file to one cause, and for `Matches.ini` the cause was something else entirely. All four are fixed in the fork (`XXMI-CHANGES.md` entries 1–4, each with a `test/corpus/xxmi.txt` case), and all 19 files now parse without ERROR or MISSING nodes.

| File | Suspected | Actual cause | DLL source | Fork fix |
|---|---|---|---|---|
| `Core/ZZMI/Libraries/HP bar/hp.ini` | `store = $x, Resource.UAV, 7`, `>>`, `&` | The external scanner read `Resource.UAV,` (with the comma) as the resource name. `>>` and `&` were fine | `CommandArgumentReader::GetTokenInternal` stops tokens at `,` | `,` ends a custom resource identifier (`src/scanner.c`) |
| `Core/ZZMI/Libraries/SlotFix/Matches.ini` | multi-segment `namespace` | `_resource_format` was `/DXGI_FORMAT_.+/`, eating `)`/`\|\|` to the end of the line. The namespace was fine | `tokenise()` → `FindIdentifierTokenEnd` (`[a-z0-9_]`) | `DXGI_FORMAT_[a-z0-9_]+`, same for `D3D11_BIND_` |
| `Core/ZZMI/Libraries/SlotFix/SlotFix.ini` | `$Poolt[*] = …` | `Poolt = null`: a pool without an index. The wildcard was fine | `ResourceCopyTarget::ParseTargetPool` (no index → whole pool) | bare pool is a resource operand (also fixes `PoolFoo->Size`) |
| `Core/ZZMI/Libraries/TTLib/UIPlacement.ini` | `->Offset`, `->Region`, `ref PoolX[$slot]` | only `copy vb0->Region(...)`: a member suffix on a copy source | `ResourceCopyTarget::ParseTarget` → `ParseTargetMember` | `resource_usage_expression` accepts `property_access_expression` |

Minimal license-clean reproductions live in `fixtures/parser/xxmi-dialect/`.

Upstream 0.9.4 (merged into the fork 2026-10-19) fixed all four on its own; the fork keeps only an alias so a bare pool is still a `resource_pool` node, which the model reads as a pool reference. The fork also adds slot expressions and slot/pool ranges (`ps-t[$i]`, `ps-t[$a:$b]`, `PoolFoo[$a:$b]`; `ResourceCopyTarget::ParseTargetSlotExpression` and `ParseRangeBounds`, XXMI-Libs-Package PR #55) as `XXMI-CHANGES.md` entry 5 (`fixtures/parser/xxmi-dialect/slot-pool-ranges.ini`).

### Error recovery

tree-sitter's recovery is heuristic. Most errors stay local, but some swallow the rest of the file: parsing a whole file, an unclosed `(` in `$y = ($x + 1` becomes one ERROR node that also covers every later section (`fixtures/parser/errors/unclosed-paren.ini`).

Lowering avoids this (M2). `scan.rs` (`scan_ini`) finds sections exactly the way the DLL's `ParseIniStream` does (a trimmed line starting with `[` is a header, `;` comments only at the start of a line, `key = value` split at the first `=`). Each section, and the preamble, is then parsed on its own (`document.rs` parses the section's text slice), keeping absolute positions. A syntax error can't cross a section boundary, which also matches the DLL, which parses sections independently.

### Parser contract for core

- `parse(text) → Tree` (incremental: `tree.edit` plus reparse on LSP `didChange`).
- Implemented in M0 (`rust/crates/xxmi-core/src/parse.rs`): `new_parser` builds the tree-sitter parser, and `collect_syntax_issues` returns the ERROR/MISSING nodes as `SyntaxIssue`s (outermost ERROR only, plus every MISSING).
- A CST-to-model lowering pass (`model/lower.rs`, `lower_ini`) builds the typed model below and never throws. ERROR and MISSING nodes become `XM001` diagnostics (see `02-diagnostics.md`), and lowering continues with the next section.
- Keep a thin fallback line scanner: section headers plus `key = value` split.

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

- `\` and `/` are both separators in `include =`, `filename =` and similar keys. Never pass an ini path straight to the OS path APIs; use `rust/crates/xxmi-core/src/paths.rs`, which normalizes them.
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

- **Roots:** the opened folder, plus the **package root**. The package root comes from `xxmi.toml` (`package = "../../"`) or is auto-detected by walking up from the mod to the nearest folder containing `d3dx.ini`. In a typical XXMI install, `ZZMI/Mods/<mod>/` walks up to `ZZMI/`.
- **Includes:** follow `[Include*]` `include`, `include_recursive` and `exclude_recursive` like the DLL does, including the recursive `Mods/` include from `d3dx.ini`. Files are indexed lazily: open files first, then include targets, then the rest in the background.
- **Bundled library snapshot:** each build of `xxmi-core` embeds a pre-built index of the libraries of each game profile that has one (symbols and docs only; see §5). A user with just a mod folder still gets completion and hover for `CommandList\ZZMI\…`. A local package, when found, overrides the snapshot.
- **As built (M2, `rust/crates/xxmi-core/src/workspace/`):**
  - The package root is found by walking up to a folder with `d3dx.ini`; `xxmi.toml` `package =` is not read yet.
  - When a root is found, `d3dx.ini` (namespace `''`) and everything it `include`s are loaded, transitively. `include_recursive` folders (`Mods/`) are **not** bulk-loaded; only the files being linted are, each with the namespace the DLL would give it (its path from the root, `\`-separated).
  - Without a root, targets get path namespaces relative to the lint folder, which keeps them unique within the run.
  - `exclude_recursive` globs from `d3dx.ini` `[Include]` (ZZMI: `DISABLED*`, `desktop.ini`) are matched like `ParseIniFilesRecursive` does: caseless PCRE2 globs against each file and folder name below the package root (`DISABLED*` when there is no package). Linting a folder and the LSP's background index skip matching files. A file opened or linted on its own is still checked, but XM107 doesn't compare it with other files, since the DLL never loads it next to them. Files reached through `include =` are never excluded.
  - Symlinks: each path is its own file (the DLL loads both when both are walked), but an editor buffer applies to every loaded path of the same file on disk, so a library symlinked into `Core/` follows edits to its working copy.
  - Library references then resolve through the snapshot of the package's profile, `snapshots/<id>.json`, built with `pnpm xxmi index <package> --out snapshots/<id>.json --name <id>`. A snapshot is used only when no loaded file declares any of its namespaces. CI checks every snapshot matches its pinned package commit.
  - Include-derived namespaces use the on-disk spelling and `\`; the DLL keeps the path as written in `include =`. This only matters for explicit references to path namespaces, which libraries don't use (they declare `namespace =`).
- **Invalidation:** file watcher (LSP `workspace/didChangeWatchedFiles`). Re-lower only changed files, then re-resolve references in files that depend on changed symbols.

## 5. Game profiles

As built (`rust/crates/xxmi-core/src/profile/`):

- **Profiles:** `xxmi` (generic: XXMI dialect, no library), `gimi` (GIMI dialect), and `srmi`, `wwmi`, `zzmi`, `himi`, `efmi` (XXMI dialect). Each of the last six has a library snapshot, `snapshots/<id>.json`, pinned to its package commit. `xxmi capabilities --json` lists them.
- **Why `gimi` has its own dialect:** many Genshin players can't use the XXMI DLL because of its anti-cheat, so they run the GIMI DLL (XXMI-Libs-Package v0.1.0 is vanilla GIMI code). The latest GIMI-Package still works with it, and some players do run the XXMI DLL.
- **A package** is the folder with `d3dx.ini` nearest above a file, i.e. one MI install. Files with none form one implicit package. Each package is checked against its own profile's library only. Symbols never resolve across packages in one run, but mods in the same package see each other.
- **Which profile a package uses**, first match wins:
  1. `--mi-package <id>` (CLI) or the `xxmi.miPackage` editor setting, unless it is `auto`.
  2. `profile = "<id>"` in the `xxmi.toml` at or above the package root.
  3. A package folder named after one of the six MI ids (`gimi`, `srmi`, `wwmi`, `zzmi`, `himi`, `efmi`; case-insensitive), or one holding `Core/<MI>/`.
  4. References to a library namespace that only one profile's snapshot declares.
  5. `xxmi`.

Not implemented: the earlier `spec/profiles/zzmi.toml` sketch (format eras, hash sources, core namespace). The registry holds only id, name, dialect and snapshot.
