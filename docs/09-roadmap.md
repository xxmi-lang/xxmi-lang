# 09 — Roadmap

Milestones are ordered by value. Linter and LSP come first, because they help humans and agents at once. Each milestone lists tasks and **acceptance criteria**. Don't start the next milestone until the current one's criteria pass.

## M0: Bootstrap and parser validation

- Scaffold the monorepo per `00-overview.md` (pnpm workspaces, TS strict, vitest, eslint, prettier, GPL-3.0 LICENSE, CI on GitHub Actions for Windows, Linux and macOS).
- `packages/core/parser`: load `tree-sitter-migoto` WASM through `web-tree-sitter`. Write a `parseFile` helper that reports ERROR/MISSING nodes.
- `pnpm test:corpus`: parse every `.ini` under `corpus/` and under a configured ZZMI-Package checkout, then print the failure rate and the first error per file.
- Fork `lupomikti/tree-sitter-migoto` per the fork policy in `01-language-model.md` §2, with the monthly upstream-merge job.
- Reproduce the 4 known dialect gaps (`01-language-model.md`). For each: minimal corpus test and grammar fix in the fork, logged in `XXMI-CHANGES.md`.

**Accept:** 19/19 ZZMI-Package `.ini` files parse without ERROR nodes; corpus parse-failure rate recorded as a baseline.

**Status (2026-09-30):** done. Repos: [`xxmi-lang/xxmi-lang`](https://github.com/xxmi-lang/xxmi-lang) and the grammar fork [`xxmi-lang/tree-sitter-migoto`](https://github.com/xxmi-lang/tree-sitter-migoto) (default branch `xxmi`).

- ZZMI-Package @ `1ecd24d`: 19/19 files parse without ERROR or MISSING nodes (`pnpm test:corpus --strict`, also run in CI on all three OSes; weekly against ZZMI `main`).
- Local corpus baseline (`corpus/.baseline.json`): 3/3 files clean (0.0% parse-failure rate), 1 not UTF-8. The corpus is still tiny; grow it with the CN mods requested for M4.
- Grammar fork: `xxmi` branch on top of upstream v0.9.3, 4 fixes in `XXMI-CHANGES.md` (root causes in `01-language-model.md` §2). Vendored WASM provenance is in `packages/core/grammar/SOURCE.json`.
- Open: the org setting "Allow GitHub Actions to create and approve pull requests" is off, so the fork's `upstream-sync` job can push its branch but can't open the PR until an org owner enables it.
- Carried into M2: error recovery can swallow later sections after an unclosed `(` (`01-language-model.md` §2 "Error recovery").

## M1: Spec extractor

- `packages/spec-extractor`: given `XXMI_DLL_SRC`, produce `spec/generated/{sections,commands,operators,meta}.json` per `01-language-model.md` §1. Tolerate source drift: find arrays and functions by name, and fail loudly if not found.
- Overlay loader and merger; initial overlay docs for the 20 most used keys and commands.
- CI job: weekly, plus manual dispatch. Clone the DLL, run extraction, open a PR on diff.

**Accept:** every section prefix and operator in the DLL tables appears in the generated spec with a `source`; `store`, `local`, Pool and `->` features are present and flagged `xxmi: true`; rerunning on the same commit gives byte-identical output.

**Status (2026-09-30):** done, with one correction to the criteria. `local` is present but `xxmi: false`, because vanilla 3DMigoto already parses `local $x` (`ParseCommandListVariableAssignment`: `name.compare(0, 6, L"local ")`), and the DLL wins over this doc. `store`, Pool (the section and all its keys), all 17 `->` members, `locked`, the shift/bitwise operators and the math functions are flagged `xxmi: true`.

- `spec/generated/` is from XXMI-Libs-Package @ `34fa32f` with 3Dmigoto @ `8f329bd` as the vanilla baseline: 28 sections, 32 commands, 17 resource members, 22 functions, 26 operator tokens in 12 precedence levels, 26 enum tables. Two runs are byte-identical, including from CRLF sources.
- CI checks that `spec/generated/` still matches the DLL commits in `meta.json` (`pnpm spec:extract --check`) and runs the real-DLL integration tests. `spec-extract.yml` re-extracts weekly from both `master`s and opens a PR on diff.
- Overlays (`spec/overlay/commands.toml`, `sections.toml`) document the 20 most used commands and keys (ranked over ZZMI-Package and the local corpus): `run`, `if`/`elif`/`else`/`endif`, `global` (+ `persist`, `locked`), `local`, `checktextureoverride`, `clear`, `draw`, `drawindexed`, `handling`, `store`, and Resource `type`/`format`/`filename`/`stride`, TextureOverride `hash`/`match_priority`, ShaderOverride `hash`, and Include `include`/`include_recursive`/`exclude_recursive`. Every statement was checked against the DLL source.
- Found for later milestones: recursive include only loads names ending in lowercase `.ini`, so `Mod.INI` is skipped silently (a candidate rule); XXMI reads no `[Stereo]` keys.

## M2: Core model, index and `xxmi lint`

- CST → model lowering; namespace resolution rules 1–7 with a fixture for each.
- Workspace index with package-root detection and include following.
- Rules: XM001, XM002, XM101–XM109, XM201–XM208.
- `xxmi lint` with text/json output and exit codes (`04-cli.md`).
- Bundled ZZMI library snapshot built from the package (`xxmi index`).

**Accept:** linting the ZZMI package itself reports no false-positive errors (any real errors found get reported upstream); every rule has good/bad fixtures; lint of the full corpus finishes without crashes; JSON output validates against its schema.

**Status (2026-09-30):** done.

- **ZZMI-Package lint (@ `1ecd24d`):** 0 errors, 17 warnings, each checked against the DLL source.
  - 15 × XM102: keys XXMI doesn't read. 12 of them vanilla 3DMigoto reads but XXMI dropped (stereo options, `mode`, `take_screenshot`, …); the other 3 no version reads (`[Stereo] unlock_*`, `[Logging] convergence`/`separation`).
  - 2 × XM208: `Core/ZZMI/main.ini` points at `MyText.txt` and `Notifications/ErrorOldVersionZZMI.md`, which the package doesn't ship.
  - None of these are errors; the two missing files are worth reporting to the package maintainers.
- **Extractor fixes it took to get there:**
  - Keys read by the Injector and `DLLMainHook.cpp` through `find_ini_*_lite`, and by the D3DCompiler wrapper, were missing and caused false XM102s on `[Loader]` and `[Rendering]`; they're now extracted.
  - Missing `include_recursive` folders are no longer XM207: the DLL only logs them.
- **Fixtures:** every rule has good/bad fixtures (`fixtures/rules/`), and namespace rules 1–7 each have one with hand-written expectations (`fixtures/namespaces/`).
- **Corpus and schema:** the corpus lints with no crashes (per-rule counts are in `corpus/.baseline.json`), and CLI JSON output validates against `packages/cli/schema/lint-output.v1.json` in tests.
- **Error recovery:** the M0 gap is closed. Sections are parsed independently, so a syntax error can't hide later sections.
- **Performance:** the whole ZZMI package (19 files) lints in about 110 ms.
- **Snapshot:** the bundled ZZMI library snapshot (`packages/core/snapshots/zzmi.json`) is checked in CI against the pinned package commit.
- **Not done yet:** `xxmi.toml` `package =`, SARIF output, `--fix`, and loading other mods under `include_recursive` for cross-mod references.

## M3: LSP MVP and first release (v0.1)

- `xxmi-lsp` with the phase 1 capabilities in `03-lsp.md`.
- VS Code client extension; Kate config snippet; Claude Code plugin with `.lsp.json` and the `xxmi-ini-authoring` skill.
- Single-file binaries for every target listed in `00-overview.md` via `bun build --compile`, attached to GitHub releases. The grammar `.wasm` must be embedded in the binary (not loaded from a path next to it); test the compiled binary, not just `node dist/`.
- README: install steps for each editor, credits to lupomikti and the XXMI/AGMG maintainers.

**Accept:** in VS Code and Kate on a real ZZZ mod: diagnostics appear, go-to-definition into ZZMI libraries works, and renaming a CommandList updates all references across files. In Claude Code, an edit that introduces an unresolved `run =` produces an XM201 diagnostic in the agent's context. `claude plugin validate ./plugin` passes. The performance targets in `03-lsp.md` hold.

## M4: Formatting and normalization

- `xxmi fmt` plus LSP formatting.
- `normalize` steps 1–5 (`05-transforms.md`), with golden fixtures per step.
- Skills: `xxmi-normalize-mod`, `xxmi-apply-library`.
- Rules XM4xx, XM301–XM303.

**Needs from maintainers:** 3–5 CN mods for the local corpus, one hand-cleaned target, and the ZZZ slot-name mapping.

**Accept:** `fmt` is idempotent on the whole corpus. On the hand-cleaned target mod, `normalize` gets within the agreed diff of the hand-cleaned version (define "agreed" with the maintainer: e.g. all renames correct or flagged for review). No semantic change: symbol reference graph before equals after, modulo renames.

## M5: Migration

- Recipe framework, era detection, `xxmi migrate`, LSP code action, XM501/XM502.
- First real recipe, built from maintainer-provided before/after pairs.
- Skill: `xxmi-migrate-mod`.

**Needs from maintainers:** era definitions and 3–5 before/after pairs per transition (`05-transforms.md`).

**Accept:** for each provided pair, `migrate` on "before" equals "after", or differs only in items listed as `manual` in the report. Era detection is correct on all provided examples.

## M6: Hosted MCP on Railway

- `packages/mcp-server` per `06-mcp-server.md`; Dockerfile; `railway.toml`; volume; polling indexer.
- `.mcp.json` added to the plugin; `AGENTS.md`.

**Accept:** a fresh Claude Code session with only the MCP configured (no local clone) can answer "what inputs does `CommandList\ZZMI\SetTextures` need", "where is `store` implemented in the DLL" and "lint this ini" correctly. A new ZZMI-Package commit is indexed within 20 minutes. Rate limiting and input caps are verified by tests.

## M7: Polish and ecosystem

- LSP phase 2 capabilities (semantic tokens, workspace symbols, folding, inlay hints, signature help).
- Kate syntax PRs to lupomikti's `3dmigoto-ini-extension` for the XXMI dialect (`08-editor-integration.md`, "Kate syntax updates"). Can start as soon as M1's spec exists; it doesn't depend on the LSP.
- Kate `hlsl.xml`, then offer it to lupomikti's `kate-plugin` and/or KDE.
- **XXMITools template CI:** in XXMITools (PR to leotorrez), add a job that renders each `templates/*.ini.j2` with fixture contexts (derive the context shape from where `migoto/exporter.py` renders the template), then runs `xxmi lint` on the output.
- Propose the `;;` doc-comment convention to ZZMI maintainers, with a PR annotating the public API of SlotFix and TTLib.
- Additional game profiles (GIMI, SRMI, WWMI, HIMI) as data.
- Plugin evals (`07-agent-integration.md`).

## Open questions (resolve with maintainers as they come up)

1. Official names: `xxmi-lang` / `xxmi` OK? npm scope? Domain for the MCP server?
2. Which DLL version is the minimum supported? Do we warn on features newer than the user's installed DLL?
3. Component hash mapping source for ZZZ: XXMITools `hash_json.py` data, a community DB, or something new?
4. Should `normalize` ever touch a mod's `[Key*]` bindings, or leave user-facing toggles strictly alone?
