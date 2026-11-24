# 04 — CLI (`xxmi`)

The CLI is the workhorse for agents, CI and batch work. `xxmi lint` reads `xxmi.toml`, and `query` and `capabilities` take `--json`. Every command uses the exit codes below. Skills call these commands; they never re-derive logic. What tools that embed `xxmi` can rely on is listed in `11-compatibility.md`.

## Commands

```
xxmi lint [paths…]            Lint files/folders. Default path: .
    --format text|json        text (default), or the versioned JSON (see Output)
    --rules XM201,XM202       only these
    --max-warnings N          exit 1 if exceeded
    --require-package         skip files not inside a package (no d3dx.ini in a parent folder)
    --mi-package ID           game profile for every package: xxmi, gimi, srmi, wwmi, zzmi, himi, efmi (default: detect per package)
    --spec FILE               use this spec.json instead of the built-in one
    --stdin --stdin-path P    lint the text on stdin as if it were the file P

xxmi query <kind> [name]      Read-only lookups (used by skills and MCP)
    kinds: section-kinds | keys <section> | command <name> | operator <op>
           symbol <qualifiedName> | library [name]
    --package DIR             package that symbol and library read (default: the current folder)
    --json

xxmi index <package dir> --out FILE [--name NAME]
                              Build a symbol index snapshot (used for the bundled library snapshot and MCP)

xxmi lsp [--stdio]            The language server (same as `xxmi-lsp`; stdio is the only transport)

xxmi capabilities [--json]    Rules, profiles, lint schema versions and version of this build (see 11-compatibility.md)

xxmi --version | -v           Print the bare version
xxmi --help | -h              Print the usage; each command also takes --help
```

### Implemented so far (M2)

- `xxmi lint` with `--format text|json`, `--rules`, `--max-warnings`, `--require-package` and `--spec`. Not yet: `sarif`, `--fix`, and the `--json` alias. `xxmi lint --help` lists every rule.
- `xxmi lint --mi-package <id>` checks every package against that profile's library instead of detecting it. An unknown id is a usage error (exit 2) that lists the valid ids. The detection order is in `01-language-model.md` §5.
- A folder is linted like the DLL walks `include_recursive`: files and folders matching the package's `exclude_recursive` globs (`DISABLED*`, `desktop.ini` in ZZMI) are skipped. Name a disabled file or folder directly to lint it anyway.
- `xxmi lint --stdin --stdin-path <path>` lints one file's text from stdin as if it were the file at `<path>`. The path decides the namespace and the package (the nearest `d3dx.ini` above it), exactly as for `xxmi lint <path>`, and does not have to exist; the text wins over what is on disk. The output is the usual text or JSON for that one path. Like linting a single file, it loads that file and the package's `d3dx.ini` includes, not the other files of the mod folder. Combining `--stdin` with paths, `--stdin` without `--stdin-path`, or `--stdin-path` alone is a usage error (exit 2). The text may be UTF-8 with or without a BOM. It uses the workspace's editor-buffer overlay (`Workspace.openDocument`), the same as the language server, so editors and the .NET library (`10-dotnet.md`, `LintTextAsync`) can check unsaved text with one process.
- `xxmi index <package dir> --out FILE [--name NAME]`.
- `fmt`, `normalize` and `migrate` are not part of this CLI. They moved to the maintainer's C# app (`05-transforms.md`).
- `xxmi lsp [--stdio]`: starts the same language server as `xxmi-lsp` (`03-lsp.md`) on stdin/stdout, so one binary can serve both; the server code is loaded only for this command. `xxmi-lsp` keeps working. The .NET library (`10-dotnet.md`) uses it. CI runs the LSP protocol tests against `xxmi lsp` of the compiled binary (`XXMI_LSP_COMMAND`).
- `xxmi query <kind> [name] [--package DIR] [--json]` (M6): `section-kinds`, `keys <section>`, `command <name>` (also finds `->members` and functions), `operator <op>`, `symbol <qualifiedName>` and `library [name]`. The first four read the bundled spec. `symbol` and `library` read the package around `--package` (default: the current folder), else the bundled ZZMI snapshot, which has locations but no doc comments. Exit 1 when nothing is found. Not implemented: `refs`, `outline`. The logic is in `rust/crates/xxmi-core/src/query/`, shared with the MCP server.
- `xxmi capabilities [--json]`: the rules of this build (id, default severity, title, quick fix), the game profiles, the lint schema versions and the binary version. The contract is in `11-compatibility.md`.
- `pnpm spec:extract` runs the separate `xxmi-spec-extract` binary, not `xxmi`. From the repo: `pnpm xxmi …` runs the CLI from source.

## Output

- **Text:** `path:line:col  severity  XM201  message  [fix available]`, with 1-based line and column. Paths are relative to the cwd. The last line is a summary such as `2 files: 1 error, 0 warnings, 0 info, 0 hints`. This is concise on purpose so agents can read it cheaply.
- **JSON:** `xxmi lint --format json` prints `{ "version": 1, "files": [{ "path", "encoding", "diagnostics": [{ "id","severity","message","range","fix"? }] }], "summary": {…} }`. The schema is `schema/lint-output.v1.json`; units, order and the freeze are in `11-compatibility.md`. `query --json` prints JSON too, but it is not part of that contract.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success. Warnings don't change the code unless `--max-warnings` is exceeded. |
| 1 | Lint errors found, or `--max-warnings` exceeded. `query` found nothing. |
| 2 | Usage error: no arguments (prints usage), unknown command or flag, invalid value, missing path. |
| 3 | Internal error or panic (bug; report it). |
