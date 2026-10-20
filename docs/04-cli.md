# 04 — CLI (`xxmi`)

The CLI is the workhorse for agents, CI and batch work. Every command reads `xxmi.toml`, supports `--json`, and uses the exit codes below. Skills call these commands; they never re-derive logic.

## Commands

```
xxmi lint [paths…]            Lint files/folders. Default path: .
    --format text|json|sarif  sarif for GitHub code scanning
    --rules XM201,XM202       only these
    --fix                     apply safe quick fixes in place
    --max-warnings N          exit 1 if exceeded
    --stdin --stdin-path P    lint the text on stdin as if it were the file P

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

xxmi lsp [--stdio]            The language server (same as `xxmi-lsp`; stdio is the only transport)

xxmi spec extract --dll DIR   Same as `pnpm spec:extract`, for maintainers
```

### Implemented so far (M2)

- `xxmi lint` with `--format text|json`, `--rules` and `--max-warnings`. Not yet: `sarif`, `--fix`, and the `--json` alias. `xxmi lint --help` lists every rule.
- A folder is linted like the DLL walks `include_recursive`: files and folders matching the package's `exclude_recursive` globs (`DISABLED*`, `desktop.ini` in ZZMI) are skipped. Name a disabled file or folder directly to lint it anyway.
- `xxmi lint --stdin --stdin-path <path>` lints one file's text from stdin as if it were the file at `<path>`. The path decides the namespace and the package (the nearest `d3dx.ini` above it), exactly as for `xxmi lint <path>`, and does not have to exist; the text wins over what is on disk. The output is the usual text or JSON for that one path. Like linting a single file, it loads that file and the package's `d3dx.ini` includes, not the other files of the mod folder. Combining `--stdin` with paths, `--stdin` without `--stdin-path`, or `--stdin-path` alone is a usage error (exit 2). The text may be UTF-8 with or without a BOM. It uses the workspace's editor-buffer overlay (`Workspace.openDocument`), the same as the language server, so editors and the .NET library (`10-dotnet.md`, `LintTextAsync`) can check unsaved text with one process.
- `xxmi index <package dir> --out FILE [--name NAME]`.
- Not planned here any more: `fmt`, `normalize` and `migrate` (`05-transforms.md`, superseded by the maintainer's C# app).
- `xxmi lsp [--stdio]`: starts the same language server as `xxmi-lsp` (`03-lsp.md`) on stdin/stdout, so one binary can serve both; the server code is loaded only for this command. `xxmi-lsp` keeps working. The .NET library (`10-dotnet.md`) uses it. CI runs the LSP protocol tests against `xxmi lsp` of the compiled binary (`XXMI_LSP_COMMAND`).
- `xxmi query <kind> [name] [--package DIR] [--json]` (M6): `section-kinds`, `keys <section>`, `command <name>` (also finds `->members` and functions), `operator <op>`, `symbol <qualifiedName>` and `library [name]`. The first four read the bundled spec. `symbol` and `library` read the package around `--package` (default: the current folder), else the bundled ZZMI snapshot, which has locations but no doc comments. Exit 1 when nothing is found. Not implemented: `refs`, `outline`. The logic is in `packages/core/src/query/`, shared with the MCP server.
- `xxmi spec extract` is still `pnpm spec:extract`. From the repo: `pnpm xxmi …` runs the CLI from source.

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
