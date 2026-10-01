# 07 — Agent integration

## Claude Code plugin (`plugin/`)

```
plugin/
├── .claude-plugin/plugin.json
├── .lsp.json                    # xxmi-lsp for .ini; diagnostics pushed into context after edits
├── .mcp.json                    # hosted MCP server (HTTP)
├── hooks/hooks.json             # optional fallback lint hook (see below)
└── skills/
    ├── xxmi-ini-authoring/SKILL.md
    ├── xxmi-migrate-mod/SKILL.md
    ├── xxmi-normalize-mod/SKILL.md
    ├── xxmi-apply-library/SKILL.md
    └── xxmi-dll-dev/SKILL.md
```

### `.lsp.json`

Claude Code plugins support LSP servers natively. With `diagnostics` on (the default), diagnostics are pushed into the agent's context after each edit, which is exactly the feedback loop we want. `command` and `extensionToLanguage` are required; the config object is strict, so unknown keys fail validation.

```json
{
  "xxmi": {
    "command": "xxmi-lsp",
    "args": ["--stdio"],
    "extensionToLanguage": { ".ini": "migoto" },
    "startupTimeout": 10000
  }
}
```

- **Binary:** v1 requires `xxmi-lsp` on PATH (release binary or `npm i -g @xxmi-lang/cli`). The README says so, and the `xxmi-ini-authoring` skill tells the agent to check `xxmi --version` and explain installation if it's missing. Later: a launcher that downloads the platform binary into `${CLAUDE_PLUGIN_DATA}`.
- **Scope:** mapping all `.ini` files is fine because users enable the plugin per project (mod or package folders). The README recommends project-scope enablement.
- Validate with `claude plugin validate ./plugin` in CI.

### As built (M3)

- **Shipped:** `plugin/.claude-plugin/plugin.json`, `plugin/.lsp.json` (as above) and the `xxmi-ini-authoring` skill. `.claude-plugin/marketplace.json` at the repo root makes the repo a marketplace: `claude plugin marketplace add xxmi-lang/xxmi-lang`, then `claude plugin install xxmi@xxmi-lang`. CI runs `claude plugin validate` on both.
- **Not shipped yet:** `.mcp.json` (M6), the fallback hook and the other skills (M4/M5). The authoring skill only names commands that exist (`xxmi lint`, `--version`); `xxmi query` arrives with the MCP work.
- **Verified 2026-10-01:** a headless session (`claude -p --plugin-dir ./plugin`, `xxmi-lsp` on PATH) edited `run = CommandListLocal` to `CommandListLocl`, and the agent reported "XM201, line 5:7, Unresolved command list `CommandListLocl`. Did you mean `CommandListLocal`?"
- **Timing:** Claude Code starts plugin LSP servers lazily, on the first edit to a matching file, and attaches diagnostics to the agent's *next* turn. If the agent answers immediately after its first edit, the diagnostic can arrive after it has answered. Any further step delivers it.

### `.mcp.json`

```json
{ "mcpServers": { "xxmi": { "type": "http", "url": "https://<your-domain>/mcp" } } }
```

### Fallback hook

Only for setups where the LSP can't run. A `PostToolUse` hook matching `Write|Edit` runs `xxmi lint --format text <file>` on edited `.ini` files, and returns the output to the agent when there are errors. Ship it disabled by default (document how to enable), since the LSP already covers this and running both would duplicate messages.

## Skills

Skills hold **workflow and judgment**. Facts come from the CLI, LSP or MCP, so skills stay short and don't go stale when the DLL changes. Each skill: frontmatter `name` + `description` (when to use it), then steps. Reference CLI commands explicitly.

### `xxmi-ini-authoring` (base skill, used by the others)

- Before reading library source, use `xxmi query library` / `xxmi query symbol` (local) or MCP `library_symbol` (remote).
- After every edit, fix all XM0xx/XM1xx/XM2xx errors before continuing. Don't suppress rules without a comment saying why.
- Conventions: namespace declared at the top of every new file; doc comments (`;;` tags) on every new CommandList; don't write to `locked` library globals.
- How to check that the tooling is installed, and what to say if it isn't.

### `xxmi-migrate-mod`

1. `xxmi migrate --detect`. If confidence is low, show the evidence and ask the user which era it is.
2. Make sure the mod is in git (or let the CLI back it up).
3. `xxmi migrate --to <latest> --report`.
4. For each `manual` item in the report, follow the explanation from the recipe. Show the user the proposed change for anything that affects visuals (shape keys, vertex limits, blend weights).
5. `xxmi lint` must be clean. Summarize the changes, and list what should be checked in game.

### `xxmi-normalize-mod` (CN and messy mods)

1. `xxmi normalize --dry-run --report`. Show the encoding detection and rename plan.
2. Review the rename plan. Improve names the heuristics got wrong by writing a `--rename-map` (based on texture filenames, what the override draws, and the ZZZ slot names).
3. Apply. Then translate non-English comments (the skill step), keeping the originals only if the user asks.
4. Look for hand-rolled code that a ZZMI library replaces (handoff to `xxmi-apply-library`).
5. Lint clean. Produce a short summary: the mod's components, toggles, and keys.
6. If the mod is encrypted or the author forbids modification, stop and tell the user.

### `xxmi-apply-library`

1. `xxmi query library` → pick a library. Read its module doc and the target symbol's `@in` tags.
2. Find the call sites in the mod (the LSP's references, or `xxmi query refs`).
3. Replace the hand-rolled code with the library call: set inputs, then `run = CommandList\<ns>\<Name>`.
4. Lint (XM302 catches missing inputs). Note the minimum package version in the mod's header comment.

### `xxmi-dll-dev` (for work on the XXMI-Libs-Package fork)

- Map from ini feature to source: `find_implementation` via MCP, or the `source` field in `spec/generated`.
- Adding an ini feature: parse in `IniHandler.cpp`/`CommandList.cpp` → rerun `xxmi spec extract` → add overlay docs → add a tree-sitter-migoto grammar test and fix → add lint fixtures.
- Build notes for VS2022 (the fork targets it); the agent shouldn't try to build on Linux unless the user has a cross-setup.

## Other agents

- Agent Skills format: other agents that support the format can load `plugin/skills/*` directly.
- `AGENTS.md` at the repo root: a short version of `CLAUDE.md`'s rules, plus "use `xxmi` CLI and the hosted MCP", for Codex, Cursor, etc.
- The MCP server is the universal entry point for agents without shell access.

## Evaluation

`plugin/evals/` (Claude Code `claude plugin eval` format): cases like "fix this broken mod" (fixture with 5 seeded errors → lint clean), "migrate fixture X" (compare to golden output), "what inputs does `CommandList\ZZMI\SetTextures` need" (must answer from the tool, not by reading the whole package). Run before each plugin release.
