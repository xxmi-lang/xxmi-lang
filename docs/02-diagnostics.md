# 02 — Diagnostics

Every rule has a stable ID, a default severity, a description, fixtures, and optionally a quick fix. The LSP, CLI and MCP `lint_ini` all run the same rule engine from `@xxmi-lang/core`.

## ID ranges

| Range | Category |
|---|---|
| `XM0xx` | Syntax / parse |
| `XM1xx` | Structure (sections, keys, values) |
| `XM2xx` | References and namespaces |
| `XM3xx` | XXMI / game-profile specific |
| `XM4xx` | Style and readability (off by default in CLI, info in LSP) |
| `XM5xx` | Format-era / migration hints |

## Initial rule set

Build these in roughly this order. Severity: E = error, W = warning, I = info, H = hint.

| ID | Sev | Rule | Quick fix |
|---|---|---|---|
| XM001 | E | Parse error (tree-sitter ERROR/MISSING node) | — |
| XM002 | W | File isn't valid UTF-8 (likely GBK/Shift-JIS) | Convert encoding (`normalize --encoding`) |
| XM101 | E | Unknown section prefix (not in Command-list or Regular sections) | Suggest nearest prefix (edit distance) |
| XM102 | W | Unknown key for this section kind | Suggest nearest key |
| XM103 | E | Bare line (no `=`) in a section that doesn't allow it | — |
| XM104 | W | Duplicate key not in the DLL's duplicate whitelist | Remove the earlier one |
| XM105 | E | Invalid enum value (resource `type`, `format`, key `type`, …) | Suggest valid values |
| XM106 | E | Unbalanced `if`/`elif`/`else`/`endif` | Insert missing `endif` |
| XM107 | W | Duplicate section name within the same namespace | — |
| XM108 | E | Wrong arg count or shape for a command (`store`, draw commands, …) | — |
| XM109 | E | Unknown operator or malformed expression | — |
| XM201 | E | Unresolved `run = CommandList…` target | Suggest nearest symbol; offer to create the section |
| XM202 | E | Unresolved resource reference (`ref`, `copy`, `Resource\N\X`) | Suggest nearest |
| XM203 | W | Variable used but never declared (`$x` with no `global`/`local`/`persist`/Constants assignment) | Add to `[Constants]` |
| XM204 | W | Variable declared but never used | Remove |
| XM205 | I | Section defined but never referenced (CommandList, Resource, CustomShader) | — |
| XM206 | E | Unresolved namespace in `\N\name` | — |
| XM207 | E | Include target file not found | — |
| XM208 | W | `filename =` points to a missing file | — |
| XM301 | W | Writing to a `locked` library global (e.g. `$\TTL\default_alpha`) | — |
| XM302 | W | Library API used without its required inputs (from `@in` doc tags) | Insert the input lines |
| XM303 | I | Library call recognized but deprecated for this package version | Rewrite to the replacement |
| XM304 | W | Pool persistence on a non-ring index type (mirrors the DLL warning) | — |
| XM401 | I | Auto-generated or meaningless name (heuristics in `05-transforms.md`) | Rename (LSP rename) |
| XM402 | I | Very long flat command list that repeats a block ≥ N times | Extract CommandList |
| XM403 | H | Inconsistent indentation or casing | Format |
| XM501 | I | Mod detected as older format era (`position`, …) | "Run `xxmi migrate`" |
| XM502 | W | Construct known to break after a specific game or package update | Link to recipe |

Messages quote the DLL's own warning text where one exists, so users can match log output to editor output.

### As implemented (M2)

All of XM001, XM002, XM101–XM109 and XM201–XM208 are in `packages/core/src/lint/rules/`, each with `fixtures/rules/<id>/{bad,good}` and an `expected.json`. Where the DLL source settled a question, the rule follows it:

- **XM102:** unknown keys in regular sections are checked against the extracted spec. Keys vanilla 3DMigoto reads but XXMI dropped (stereo options, `mode`, …) get a message saying so. `[Profile]` is free-form. In command-list sections, only lines the grammar can't read as a command, resource copy or assignment are flagged (the DLL's "Unrecognised entry"); a misspelt keyword such as `runn` surfaces as XM001 instead.
- **XM103:** reports lines without `=` in sections that don't allow them.
- **XM104:** skips global sections in included files, which the DLL lets override the main d3dx.ini.
- **XM107:** follows DLL load order, reporting the later duplicate.
- **XM108:** uses extracted argument counts (draw commands, `store`) and skips lines that already have a syntax error.
- **XM109:** takes over from XM001 for parse errors on expression lines (`if …`, `$x = …`, `local …`).
- **XM202:** covers resource and pool references on both sides of `=`. `ParseTargetCustomResource` fails for an undeclared destination too.
- **XM203:** checks locals first (declared earlier in the same section), then `$\ns\name`, then the global name, as `parse_command_list_var_name` does.
- **XM204 / XM205:** only check files being linted, never libraries (`Core/` under a package) or d3dx.ini. For XM204, globals in those files are a library's API for other mods, so they're exempt.
- **XM207:** only covers `include =`. A missing file gives the DLL's "Error opening" overlay warning, while a missing `include_recursive` folder is only logged at info level, so it isn't reported.
- **XM208:** tries the ini's folder first, then the 3DMigoto folder, like `ParseResourceSection`, and skips absolute paths.

`pnpm test:corpus` lints every root and diffs per-rule counts against `corpus/.baseline.json` (`lint` key); with `--strict` any lint error fails.

## Configuration

`xxmi.toml` at the workspace or mod root. The nearest one up the tree wins, and settings merge with defaults.

```toml
profile = "zzmi"
package = "../.."             # optional; auto-detected otherwise

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
