# 05 — Transforms: `fmt`, `normalize`, `migrate`

All three operate on the semantic model and emit **edits** (range + new text), never whole-file rewrites. Comments and unknown content are preserved. Each is idempotent: running it twice gives the same result as running it once.

## `fmt`

Default style, configurable in `xxmi.toml [fmt]`:

- Section header on its own line, with one blank line before it. A doc comment stays attached above the header.
- `key = value` with single spaces around the first `=` only. Expression spacing normalized (`$a = $b * 2`).
- Command-list bodies: `if`/`elif`/`else`/`endif` blocks indented by one tab (the ZZMI package uses tabs); configurable to spaces.
- Keywords lowercased (`if`, `run`, `ref`, `copy`, section prefixes keep canonical case: `TextureOverride`, `CommandList`, `Resource`). User symbol names are never re-cased.
- Trailing whitespace stripped; final newline; CRLF vs LF preserved per file (the DLL doesn't care, and preserving avoids giant diffs).
- Opt-in section ordering: `[Constants]`, `[Present]`, `[Key*]`, overrides, command lists, resources. Off by default because order can matter for the DLL.

Golden tests: `fixtures/fmt/<case>/{in.ini,out.ini}`, plus an idempotence check `fmt(out) == out`.

## `normalize` (CN and messy-mod cleanup)

A pipeline of independent, individually testable steps. Deterministic steps run by default; the skill does the judgment work around them.

| # | Step | Deterministic | What it does |
|---|---|---|---|
| 1 | `encoding` | yes | Detect the encoding (UTF-8, UTF-8 BOM, GBK/GB18030, Shift-JIS, UTF-16) with `chardet`-style detection plus a validation pass. Convert to UTF-8 without BOM. Report confidence; below the threshold, stop and ask |
| 2 | `format` | yes | `fmt` |
| 3 | `rename` | yes, heuristic | Rename auto-generated or meaningless symbols. Name sources, in priority order: (a) `--rename-map`, (b) `filename =` basename (`ResourceA1B2C3 filename=Body_Diffuse.dds` → `ResourceBodyDiffuse`), (c) the TextureOverride it's bound to plus its slot (`ps-t3` → `Diffuse`, per profile slot names), (d) the component hash mapping from the profile, when available. Uses the same code path as LSP rename, so all references update |
| 4 | `dedupe` | yes | Find repeated command blocks (same normalized lines, ≥ N repeats) and extract them into `[CommandList<Name>]` with `run =` calls. Names come from the common resource or override being used |
| 5 | `group` | yes | Reorder into logical groups (Constants, Keys, overrides per component, command lists, resources per component) with `; ---- Body ----` separators. Opt-in, because ordering can matter |
| 6 | `library` | partial | Detect hand-rolled code that duplicates a ZZMI library (e.g. manual texture slot juggling that `CommandList\ZZMI\SetTextures` does) and replace it. Recipes live next to the migrate recipes |
| 7 | `translate-comments` | **no** (LLM) | Skill-only step. The CLI marks non-English comments with a report; the agent translates them |

Heuristics for "meaningless name" (also rule XM401): hex or hash-like names, `Resource1`…`ResourceN`, pinyin-only abbreviations under 4 characters, and names that differ from other names only by a counter.

**Scope:** mods that are messy but readable. If the ini is encrypted, packed, or explicitly marked by the author as not to be modified, `normalize` reports that and stops.

## `migrate` (format eras)

### Concept

Each game profile defines an ordered list of **eras**. Each consecutive pair has a **recipe**:

```ts
// packages/core/src/migrate/zzmi/position-to-blend-v1.ts
export const recipe: MigrationRecipe = {
  from: 'position', to: 'blend-v1',
  detect(model): EraEvidence { /* signals + confidence */ },
  steps: [
    { id: 'move-draw-to-blend-override', kind: 'auto', apply(model, ctx) { … } },
    { id: 'adjust-vertex-limit',          kind: 'auto', apply(…) { … } },
    { id: 'check-shape-keys',             kind: 'manual', explain: '…what to check and why…' },
  ],
};
```

`xxmi migrate` detects the current era, chains recipes up to the target, applies `auto` steps, and lists `manual` steps in the report (exit code 4). The skill walks the agent through the `manual` items.

### Era definitions for ZZMI — TO BE FILLED IN BY MAINTAINERS

The high-level history, as stated by the maintainers: early mods centered on the **Position** buffer; then they moved to **Blend** with minimal adjustments; the current Blend usage is significantly different again. The concrete, per-construct differences are not written down anywhere yet. That is the first input this project needs (see "Corpus" below).

For each era, document in `spec/profiles/zzmi/eras/<era>.md`:

1. **Detection signals:** which sections, keys, hashes or patterns identify it (e.g. "TextureOverride on the Position hash with `handling = skip` and a `draw` …").
2. **Canonical example:** a minimal mod in that era.
3. **Transition to the next era:** every change, marked auto (mechanical) or manual (needs judgment), with the reason.
4. **Pitfalls:** what commonly breaks.

Claude Code: do **not** invent era details. Implement the recipe framework, the detection plumbing and one worked recipe from the provided examples. Leave the rest as `TODO(maintainer)` with failing placeholder tests.

## Corpus and golden tests

- `corpus/` (git-ignored, local only): real mods, organized as `corpus/<source>/<mod>/`. Include `corpus/README.md` saying where each came from. Never commit it.
- `fixtures/migrate/zzmi/<from>-to-<to>/<case>/{in/, out/, report.json}`: small, hand-reduced, license-clean cases that capture each step.
- `fixtures/normalize/<step>/<case>/…`: same shape.

**Inputs the maintainers need to provide before phase 3:**

- 3–5 **before/after pairs per era transition**, migrated by hand, each with a sentence on what was changed and why.
- 3–5 **typical CN mods** (local corpus only), plus a hand-cleaned version of at least one, as the target for `normalize`.
- The **slot-name mapping** per shader stage and slot for ZZZ (`ps-t3` = Diffuse, and so on). SlotFix's `Aliases.ini` suggests the set: Diffuse, NormalMap, LightMap, MaterialMap, GlowMap, WengineFx.
- A source for **component hash mappings** (JSON from XXMITools `migoto/data/hash_json.py`? a community hash DB?), for naming and for detecting outdated hashes.
