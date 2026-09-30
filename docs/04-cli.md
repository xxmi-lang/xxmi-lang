# 04 — CLI (`xxmi`)

The CLI is the workhorse for agents, CI and batch work. Every command reads `xxmi.toml`, supports `--json`, and uses the exit codes below. Skills call these commands; they never re-derive logic.

## Commands

```
xxmi lint [paths…]            Lint files/folders. Default path: .
    --format text|json|sarif  sarif for GitHub code scanning
    --rules XM201,XM202       only these
    --fix                     apply safe quick fixes in place
    --max-warnings N          exit 1 if exceeded

xxmi fmt [paths…]             Format in place
    --check                   exit 1 if anything would change (CI)
    --diff                    print unified diff instead of writing

xxmi normalize [paths…]       CN / messy-mod cleanup pipeline (05-transforms.md)
    --steps encoding,format,rename,dedupe   subset; default all deterministic steps
    --out DIR                 write to a copy (default: in place, after backup to .xxmi-backup/)
    --rename-map map.json     explicit renames, overrides heuristics
    --report                  print what was renamed/extracted

xxmi migrate [paths…]         Upgrade format era (05-transforms.md)
    --detect                  only print detected era + confidence
    --to ERA                  default: latest for the profile
    --dry-run / --diff
    --report                  list of steps applied + items needing manual review

xxmi query <kind> [name]      Read-only lookups (used by skills and MCP)
    kinds: section-kinds | keys <section> | command <name> | operator <op>
           symbol <qualifiedName> | refs <qualifiedName> | library [name]
           outline <file>
    --json

xxmi index [dir] --out FILE   Build a symbol index snapshot (used for the bundled library snapshot and MCP)

xxmi spec extract --dll DIR   Same as `pnpm spec:extract`, for maintainers
```

## Output

- **Text:** `path:line:col  severity  XM201  message  [fix available]`. Paths are relative to the cwd. This is concise on purpose so agents can read it cheaply.
- **JSON:** `{ "version": 1, "files": [{ "path", "encoding", "era"?, "diagnostics": [{ "id","severity","message","range","fix"? }] }], "summary": {…} }`. The schema lives in `packages/cli/schema/lint-output.v1.json` and changes only with a version bump.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success, no errors (warnings allowed unless `--max-warnings`) |
| 1 | Lint errors found, `--check` found changes, or `--max-warnings` exceeded |
| 2 | Usage error |
| 3 | Internal error (bug; report it) |
| 4 | `migrate`/`normalize` finished but left items needing manual review |

## Safety for write commands

- `normalize` and `migrate` refuse to run on a dirty git working tree unless `--force`. Outside git, they back up to `.xxmi-backup/<timestamp>/` first.
- `--dry-run` is available on every command that writes.
- Never write outside the given paths.
