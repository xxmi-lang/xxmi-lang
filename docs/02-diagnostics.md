# 02 — Diagnostics

Every rule has a stable ID, a default severity, a description, fixtures, and optionally a quick fix. The LSP, CLI and MCP `lint_ini` all run the same rule engine from `xxmi-core`.

## ID ranges

| Range | Category |
|---|---|
| `XM0xx` | Syntax / parse |
| `XM1xx` | Structure (sections, keys, values) |
| `XM2xx` | References and namespaces |
| `XM3xx` | XXMI / game-profile specific |
| `XM4xx` | Style and readability (meant to be off by default in the CLI; not wired yet, see XM403) |
| `XM5xx` | Format-era / migration hints |

## Initial rule set

Build these in roughly this order. Severity: E = error, W = warning, I = info, H = hint.

| ID | Sev | Rule | Quick fix |
|---|---|---|---|
| XM001 | E | Parse error (tree-sitter ERROR/MISSING node) | — |
| XM002 | W | File isn't valid UTF-8 (likely GBK/Shift-JIS) | Convert encoding (`normalize --encoding`) |
| XM003 | W | Preamble line other than `namespace` / `condition` (the DLL ignores it) | Comment out the line |
| XM005 | W | Text after the `]` of a section header (the DLL ignores it) | Remove the text |
| XM006 | W | Section header with no closing `]` | Add the `]` |
| XM101 | E | Unknown section prefix (not in Command-list or Regular sections) | Suggest nearest prefix (edit distance) |
| XM102 | W | Unknown key for this section kind | Suggest nearest key |
| XM103 | E | Bare line (no `=`) in a section that doesn't allow it | Comment out the line |
| XM104 | W | Duplicate key not in the DLL's duplicate whitelist | Remove the earlier one |
| XM105 | E | Invalid enum value (resource `type`, `format`, key `type`, …) | Suggest valid values |
| XM106 | E | Unbalanced `if`/`elif`/`else`/`endif` | Insert missing `endif` |
| XM107 | E / W / I | Duplicate section name within the same namespace, graded by what ignoring the duplicate costs | — |
| XM108 | E | Wrong arg count or shape for a command (`store`, draw commands, …) | — |
| XM109 | E | Unknown operator or malformed expression | — |
| XM110 | W | Key with no value (the DLL ignores the line) | Comment out the line |
| XM111 | E | `if` / `elif` / `else` / `endif` outside a command list, or in the preamble | — |
| XM112 | W | TextureOverride with no `hash` and no `match_*` key, or with both; ShaderOverride with no `hash` | — |
| XM113 | E / W | Hash value the DLL can't read as hex (error), or reads from a non-canonical form (warning) | — |
| XM114 | I | TextureOverride / ShaderOverride setting written inside an `if` (the DLL reads it unconditionally) | Move it above the `if` when every branch sets it the same |
| XM115 | W | `;` comment after a value (the DLL keeps it in the value) | Move the comment above the line |
| XM201 | E | Unresolved `run = CommandList…` target | Suggest nearest symbol; offer to create the section |
| XM202 | E | Unresolved resource reference (`ref`, `copy`, `Resource\N\X`) | Suggest nearest |
| XM203 | W | Variable used but never declared (`$x` with no `global`/`local`/`persist`/Constants assignment) | Add to `[Constants]` |
| XM204 | W | Variable declared but never used | Remove |
| XM205 | I | Section defined but never referenced (CommandList, Resource, CustomShader) | — |
| XM206 | E | Unresolved namespace in `\N\name` | — |
| XM207 | E | Include target file not found | — |
| XM208 | W | `filename =` points to a missing file | — |
| XM209 | W | Global declared twice in `[Constants]` (the DLL keeps the first) | — |
| XM210 | E | Local redeclared where an earlier local of the same name is visible (the DLL drops the line) | — |
| XM211 | E | `global locked` declared again (the DLL keeps the first, so the new value never applies) | — |
| XM212 | E | Assignment to a `locked` global (the DLL rejects the line) | — |
| XM301 | W | Writing to a `locked` library global (e.g. `$\TTL\default_alpha`) | — |
| XM302 | W | Library API used without its required inputs (from `@in` doc tags) | Insert the input lines |
| XM303 | I | Library call recognized but deprecated for this package version | Rewrite to the replacement |
| XM304 | W | Pool persistence on a non-ring index type (mirrors the DLL warning) | — |
| XM305 | W | `Resource\ZZMI\<Name>` slot filled by a copy instead of `ref`, under the `zzmi` profile | Insert `ref` |
| XM306 | H | `override_byte_stride` differs from the `stride` of the `vb0` resource of the same TextureOverride, under the `zzmi` profile | — |
| XM307 | W | Syntax the GIMI DLL lacks, under the `gimi` profile (hint for d3dx.ini-style settings) | — |
| XM401 | I | Auto-generated or meaningless name (heuristics in `05-transforms.md`) | Rename (LSP rename) |
| XM402 | I | Very long flat command list that repeats a block ≥ N times | Extract CommandList |
| XM403 | H | Flow keyword (`if`, `elif`, `else`, `endif`) that isn't lowercase, or a section type not spelled as in the DLL | Lowercase the keyword; respell the section type |
| XM501 | I | Mod detected as older format era (`position`, …) | "Run `xxmi migrate`" |
| XM502 | W | Construct known to break after a specific game or package update | Link to recipe |

Messages quote the DLL's own warning text where one exists, so users can match log output to editor output.

### As implemented (M2)

All of XM001, XM002, XM101–XM109 and XM201–XM208 are in `rust/crates/xxmi-core/src/rules/`, each with `fixtures/rules/<id>/{bad,good}` and an `expected.json`. Where the DLL source settled a question, the rule follows it:

- **XM001:** leaves the lines that XM003, XM005, XM006, XM103, XM110 and XM111 name more precisely to those rules, so a line is never reported twice.
- **XM003:** the DLL's `ParseIniPreamble` reads only `namespace` and `condition` (any case) before the first section and warns "Entry outside of section" for every other line, bare lines included. Flow keywords go to XM111.
- **XM005 / XM006:** `ParseIniSectionLine` takes the name up to the first `]`, or the rest of the line when there is none, and ignores whatever follows. A `;` after the `]` is therefore harmless and not reported; other text is (warning), since it can hide a second header such as `[A][B]`.
- **XM102:** unknown keys in regular sections are checked against the extracted spec. Keys vanilla 3DMigoto reads but XXMI dropped (stereo options, `mode`, …) get a message saying so. `[Profile]` is free-form. In command-list sections, only lines the grammar can't read as a command, resource copy or assignment are flagged (the DLL's "Unrecognised entry"); a misspelt keyword such as `runn` surfaces as XM001 instead.
- **XM103:** reports lines without `=` in sections that don't allow them, with the DLL's "Malformed line" semantics (the line is dropped). A stray `else` or `endif` is XM111's.
- **XM104:** skips global sections in included files, which the DLL lets override the main d3dx.ini.
- **XM107:** follows DLL load order, reporting the later duplicate. Duplicates are only compared within one package. A prefixed section is stored as `Prefix\namespace\Name` (`get_namespaced_section_name`, `ParseIniSectionLine`) and the namespace defaults to the file's path, so two files only collide when they share a `namespace =`. `ParseIniSectionLine` keeps the first and clears the rest. A duplicate in the same file stays a warning. When the first copy is in another file the severity grades what ignoring the duplicate costs: info when it is identical to the first (keys and values equal after collapsing whitespace and lower-casing keys; comment lines don't count); error for a mesh resource (a Resource that a `vb0`-`vb2` or `ib` line binds anywhere in the namespace, `ref` and `copy` forms included, or whose `filename` ends in `.buf` or `.ib`); error for a mesh command list (a CommandList or TextureOverride with a `vb*` or `ib` line, or a TextureOverride with `handling = skip`, `draw` or `drawindexed`); warning for everything else, image resources included. The message names the first copy relative to the duplicate's folder. A severity set in `xxmi.toml` replaces the per-finding one.
- **XM108:** uses extracted argument counts (draw commands, `store`) and skips lines that already have a syntax error.
- **XM109:** takes over from XM001 for parse errors on expression lines (`if …`, `$x = …`, `local …`).
- **XM110:** the DLL drops a `key =` line with a "No value found" warning, in every section kind. XM105 skips empty values, so this rule owns them.
- **XM106:** the fix inserts `endif` after the section's last command, indented like its `if`, for each `if` left open. Stray `endif`, `elif` and `else` get no fix. The inserted line break matches the file's (CRLF stays CRLF).
- **XM111:** `ParseCommandList` only runs on the spec's `commandlist` sections (`TextureOverride`, `CommandList`, `Constants`, `Present`, `ShaderOverride`, `CustomShader`, `ShaderRegex`, `BuiltInCommandList`, `BuiltInCustomShader` and the four `Clear…View` sections), so flow keywords anywhere else, or before the first section, are never parsed. Sections with an unknown prefix are XM101's.
- **XM112:** only `hash` and the resource-description keys (`match_type` … `match_msaa_quality`) let a TextureOverride match; `filter_index`, `match_priority` and the draw-call `match_first_index`-style keys don't, so they don't exempt the section. A section that has a `hash` line, even an unreadable one, is left to XM110 and XM113. A ShaderOverride has no `match_*` path, so one without `hash` is skipped ("Section missing Hash="). Key names are matched exactly, case-insensitively, and only by presence: a key with an empty value (`match_width =`) is dropped at load ("No value found"), so it counts as absent (XM110 reports it). A TextureOverride with both `hash` and a resource key warns "Cannot use hash= and match options together!" and takes the hash path; the finding sits on each such key and says it is ignored. The keys that conflict with `hash` are the 15 resource keys: `match_type`, `match_usage`, `match_bind_flags`, `match_cpu_access_flags`, `match_misc_flags`, `match_byte_width`, `match_stride`, `match_mips`, `match_format`, `match_width`, `match_height`, `match_depth`, `match_array`, `match_msaa` and `match_msaa_quality`. The draw-call keys (`match_first_vertex`, `match_first_index`, `match_first_instance`, `match_vertex_count`, `match_index_count`, `match_instance_count`), `match_priority` and `filter_index` apply alongside `hash` and never warn.
- **XM113:** applies to every key the spec types as `hash`. The DLL reads `%16llx` and the whole value, so `0x` prefixes, short forms and a leading `+` or `-` (`%llx` follows the C standard; `-` wraps to `ffffffff…`) parse (warning), while anything else is rejected and the key counts as missing (error). TextureOverride hashes are cast to 32 bits, so more than 8 digits is an error there; other sections accept 8 or 16 digits (shader hashes are 64-bit). A severity set in `xxmi.toml` replaces the per-finding one.
- **XM114:** the keys are the section's own settings from the spec (`TextureOverride` and `ShaderOverride`): ParseCommandList skips them before it reads flow control and the `GetIni*` readers never look at `if`, so a copy inside a branch is not conditional. `handling` is a command, not a setting, so it isn't flagged. The fix applies when one chain has an `else` and every branch sets the key once to the same value: it adds one copy above the `if` (same indent) and deletes the copies. Only the first copy's diagnostic carries it, and its line break matches the file's.
- **XM115:** a `;` at the start of a value or after a blank is part of the value for the DLL, so the line is read wrong. A `;` glued to text (`a;b.dds`) is left alone. The fix's line break matches the file's. XM113 skips such values.
- **XM202:** covers resource and pool references on both sides of `=`. `ParseTargetCustomResource` fails for an undeclared destination too.
- **XM203:** checks locals first (declared earlier in the same section), then `$\ns\name`, then the global name, as `parse_command_list_var_name` does.
- **XM204 / XM205:** only check files being linted, never libraries (`Core/` under a package) or d3dx.ini. For XM204, globals in those files are a library's API for other mods, so they're exempt.
- **XM207:** only covers `include =`. A missing file gives the DLL's "Error opening" overlay warning, while a missing `include_recursive` folder is only logged at info level, so it isn't reported.
- **XM208:** tries the ini's folder first, then the 3DMigoto folder, like `ParseResourceSection`, and skips absolute paths.
- **XM209:** `ParseConstantsSection` registers globals from `[Constants]` only, and `RegisterGlobalVariable` refuses a name it already holds, so the first declaration wins (flags included, `persist` too) and the DLL warns "Redeclaration". Names compare case-insensitively and per namespace. The scope is one file; its declarations XM204 leaves to this rule.
- **XM210:** `declare_local_variable` refuses a name that the section's scope stack already holds ("Illegal redeclaration of local variable") and the line is dropped. `if` opens a scope, `elif` and `else` clear it, `endif` closes it, so sibling branches may reuse a name. A local that masks a global is only a notice there, so it isn't reported. Its declarations XM204 leaves to this rule.
- **XM211:** the first declaration wins, `locked` included, across every file of the namespace in the DLL's load order (files the DLL doesn't load don't count). When that declaration is `locked`, each later one (`global`, `persist` or `locked`) is reported, in the same file or another: its value never applies. XM209 leaves those declarations to this rule.
- **XM212:** the target of an assignment goes through `GetVariable`, which fails on a `locked` variable whatever the namespace, so the line is dropped. It covers every command-list section including `[Constants]` itself (its non-`global` lines are parsed as a command list), `$ns\x` qualified names, and `store`. A `local` of the same name wins the lookup, so assigning it is fine. `[Key…]` and `[Preset…]` sections aren't command lists, but `Override::ParseIniSection` reads each `$var = …` key through the same `GetVariable`, so a `locked` target fails there too (the value is skipped) and is reported.
- **XM305:** runs only under the `zzmi` profile. A resource copy with no `ref`, `reference`, `copy` or `copy_desc` into a custom resource is a copy (`parse_resource_copy_operation`, `CommandList.cpp`), and `ResourceCopyOperation::run` repeats it each time the line runs, so a slot set in a command list that runs per draw copies per draw. It reports `Resource\ZZMI\<Name>` (SlotFix's namespace, not `ZZMIv1`) on the left of a line in a command-list section whose source is a `Resource…` section, and only when the slot resolves to a declared section (from the library snapshot or a loaded file). `null`, slot sources and the other namespaces are left alone. The quick fix inserts `ref ` before the value, which is the form SlotFix documents; it changes a copy into a binding of the same resource, so a later change to the source shows through.
- **XM306:** runs only under the `zzmi` profile. The DLL doesn't compare the two values: `override_byte_stride` is multiplied by `override_vertex_count` into the size a matching buffer is grown to (`IniHandler.cpp`, `HackerDevice.cpp`), and `vb0` binds whatever stride its resource carries. A mismatch can be intended, so the rule is a ZZMI-Hub convention and reports only what the files settle: the TextureOverride has `override_vertex_count` (otherwise the DLL ignores the stride) and a literal `override_byte_stride`; it binds `vb0` once, outside any `if`, to a Resource that has a `filename` or `data`, one literal `stride` and no copy into it; and no command list or custom shader it runs, however deeply nested, can be unresolved or bind any `vb<n>` (a called list's buffers can't be told apart from the override's own). Library lists from the bundled snapshot (`CommandList\ZZMI\SetTextures`) hold no lines and are assumed not to bind a mod's buffers, so calling them doesn't turn the rule off. The hash can match any vertex buffer, so a TextureOverride that binds another buffer (`vb1` and up, in any branch) is skipped unless the value equals the strides of all its bound buffers added up, which gets its own message, and one with `handling = skip` is skipped too, since its stride is deliberate. The rule is a hint, not the warning ZZMI-Hub asked for, because a difference can be intended and the DLL compares nothing. A TextureOverride that binds no `vb0` (the usual vertex-limit-raise override) is not checked. There is no quick fix.
- **XM307:** runs only under the `gimi` profile, whose dialect is the GIMI DLL (XXMI-Libs-Package v0.1.0), and reports anything the spec stamps `gimi: false`, through the parser's nodes: section headers (once, with nothing for their content; the DLL warns "Unknown section" and never reads the keys, `IniHandler.cpp` L440), keys and commands (the DLL drops the line, L1993), the `locked` modifier (parsed as part of the variable name, so the declaration is rejected), resource members (`->width`), run-time values (`frame_number`, `fps`), value-form commands (`run = SnapshotInitialTextures`) and uses of a defined pool. An unknown function or operator (`abs()`, `&`, `<<`) fails the whole line (`CommandList.cpp` L3460, L3491, L3821). Keys of the sections that only configure the DLL (`Logging`, `System`, `Device`, `Rendering`, `Hunting`) are hints, since the latest GIMI-Package ships a few such settings the v0.1.0 DLL ignores. Names the spec lacks are XM101, XM102 and XM202's, never XM307's. The extractor doesn't model the `resource_copy`, `filter` and `pool` command forms, nor `BuiltIn…` section bodies and `$Pool…[i]` variables, so those are not reported. There is no quick fix.
- **XM403:** `ParseCommandList` lower-cases the whole line before `ParseCommandListFlowControl`, so `If`, `ELSE IF` and `EndIf` work and the rule is style only. Section headers are checked too: the type prefix (`[textureoverrideFoo]`, `[constants]`) is compared with the spelling in the DLL's section tables (`IniHandler.cpp:43-81`), and only the prefix is reported and fixed, because the DLL compares section names case-insensitively (`WStringInsensitiveLess`, `IniHandler.cpp:172`) and so no reference breaks. Keys, commands, values, variables and names you choose are not checked: their casing is your preference. There is no off-by-default switch for `XM4xx` yet: it runs in the CLI, the language server and the MCP server like any other rule, and `XM403 = "off"` in `xxmi.toml` turns it off, and `--rules` runs only the ids it lists.

`pnpm test:corpus` lints every root and diffs per-rule counts against `corpus/.baseline.json` (`lint` key); with `--strict` any lint error fails.

## Configuration

`xxmi.toml` at the workspace or mod root. The nearest one up the tree wins, and settings merge with defaults. `[rules]` and `profile` are read today; `profile` picks the game profile for the package (see `docs/01-language-model.md` §5).

```toml
profile = "zzmi"              # read: the game profile of the package
package = "../.."             # planned; not read yet (packages are auto-detected today)

[rules]
XM204 = "off"
XM401 = "warning"

[rules.XM402]
min_repeats = 4
```

Inline suppression: `; xxmi-disable-next-line XM204` and `; xxmi-disable XM401` … `; xxmi-enable XM401`. These are plain comments to the DLL.

## Testing

`fixtures/rules/XM201/{bad.ini, good.ini, expected.json}`. The test runner asserts exact diagnostic IDs and ranges. Every rule needs at least one `bad` and one `good` fixture. Rules with quick fixes also need `fixed.ini`, and applying the fix to `bad.ini` must equal `fixed.ini`.

Corpus regression: `pnpm test:corpus` runs every rule over `corpus/` and diffs counts per rule against `corpus/.baseline.json`. A big jump in any rule means a false-positive regression.

## Rule reference

One heading per implemented rule. The headings are the `docUrl` anchors that `xxmi capabilities --json` returns. Severities are the defaults; `xxmi.toml` can change them.

### XM001

Default severity: error. Parse error (tree-sitter ERROR/MISSING node). Quick fix: no.

### XM002

Default severity: warning. File isn't valid UTF-8 (likely GBK/Shift-JIS). Quick fix: no.

### XM003

Default severity: warning. Preamble line other than `namespace` / `condition` (the DLL ignores it). Quick fix: yes (comments the line out).

### XM005

Default severity: warning. Text after the `]` of a section header (the DLL ignores it). A `;` comment there is not reported. Quick fix: yes (removes the text).

### XM006

Default severity: warning. Section header with no closing `]` (the DLL takes the rest of the line as the name). Quick fix: yes (adds the `]`).

### XM101

Default severity: error. Unknown section prefix (not in the command-list or regular section tables). Quick fix: no.

### XM102

Default severity: warning. Unknown key for this section kind. Quick fix: yes.

### XM103

Default severity: error. Bare line (no `=`) in a section that doesn't allow it. Quick fix: yes (comments the line out).

### XM104

Default severity: warning. Duplicate key not in the DLL's duplicate whitelist. Quick fix: no.

### XM105

Default severity: error. Invalid enum value (resource `type`, `format`, key `type`, …). Quick fix: yes.

### XM106

Default severity: error. Unbalanced `if`/`elif`/`else`/`endif`. Quick fix: yes (adds the missing `endif`; other imbalances have none).

### XM107

Default severity: warning, graded per finding when the first copy is in another file: error for a mesh resource or mesh command list, info for an identical copy. Duplicate section name within the same namespace, per package. Quick fix: no.

### XM108

Default severity: error. Wrong argument count for a command (`store`, draw commands, …). Quick fix: no.

### XM109

Default severity: error. Unknown operator or malformed expression. Quick fix: no.

### XM110

Default severity: warning. Key with no value (the DLL ignores the line). Quick fix: yes (comments the line out).

### XM111

Default severity: error. `if` / `elif` / `else` / `endif` outside a command list, or in the preamble. Quick fix: no.

### XM112

Default severity: warning. TextureOverride with no `hash` and no resource-description `match_*` key, or with both (one finding per `match_*` key, which the DLL ignores); ShaderOverride with no `hash`. Quick fix: no.

### XM113

Default severity: error, lowered to warning per finding for forms the DLL accepts but that aren't canonical. Hash value the DLL can't read as hex. Quick fix: no.

### XM114

Default severity: info. TextureOverride / ShaderOverride setting written inside an `if`; the DLL reads it unconditionally. Quick fix: yes, when every branch of a chain with an `else` sets it to the same value (moves it above the `if`).

### XM115

Default severity: warning. `;` comment after a value (the DLL keeps it in the value). Quick fix: yes (moves the comment above the line).

### XM201

Default severity: error. Unresolved `run = CommandList…` / `CustomShader…` target. Quick fix: yes. Not reported when no MI was detected (the `xxmi` fallback) and a bundled library defines the name without a namespace.

### XM202

Default severity: error. Unresolved resource reference (`ref`, `copy`, `Resource\N\X`, pools). Quick fix: yes.

### XM203

Default severity: warning. Variable used but never declared. Quick fix: no.

### XM204

Default severity: warning. Variable declared but never used. Quick fix: no.

### XM205

Default severity: info. Section defined but never referenced (CommandList, Resource, CustomShader). Quick fix: no.

### XM206

Default severity: error. Unresolved namespace in `\N\name`. Quick fix: no.

### XM207

Default severity: error. Include target file not found (`include =`). Quick fix: no.

### XM208

Default severity: warning. `filename =` points to a missing file. Quick fix: no.

### XM209

Default severity: warning. Global declared twice in `[Constants]`; the DLL keeps the first. Quick fix: no.

### XM210

Default severity: error. Local redeclared where an earlier local of the same name is visible; the DLL drops the line. Quick fix: no.

### XM211

Default severity: error. `global locked` declared again; the DLL keeps the first declaration, so the new value never applies. Quick fix: no.

### XM212

Default severity: error. Assignment to a `locked` global; the DLL rejects the line. Quick fix: no.

### XM305

Default severity: warning. A `Resource\ZZMI\<Name>` slot assigned from a resource with no `ref`: the DLL copies it each time the line runs. Only reported under the `zzmi` profile. Quick fix: yes (inserts `ref `).

### XM306

Default severity: hint (a difference can be intended; the DLL never compares the two). `override_byte_stride` differs from the `stride` of the resource the same TextureOverride binds to `vb0`. Only reported under the `zzmi` profile. Quick fix: no.

### XM307

Default severity: warning (hint for keys in `Logging`, `System`, `Device`, `Rendering` and `Hunting`). Syntax the GIMI DLL (v0.1.0) lacks; only reported under the `gimi` profile. Quick fix: no.

### XM403

Default severity: hint. Flow keyword (`if`, `elif`, `else`, `endif`) that isn't lowercase, or a section header whose type (`TextureOverride`, `Resource`, `Constants`, …) isn't spelled as in the DLL; the DLL accepts any case. Keys, commands, values, variables and names you choose are not checked. Quick fix: yes (lowercases the keyword, or respells the section type and leaves the rest of the name).
