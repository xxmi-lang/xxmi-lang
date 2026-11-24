# 11 — Compatibility

This page is the contract for tools that embed `xxmi`: a bundling app, an editor integration or a CI script that runs the binary. Anything not described here may change in any release. The contract is checked by `rust/crates/xxmi/tests/contract.rs` (exit codes, version output, `capabilities --json`) and `rust/crates/xxmi-core/tests/contract.rs` (lint JSON order, `--require-package`).

## Release assets

Asset names and zip contents are part of the contract.

- `xxmi-<version>-<target>.zip`, one per target: `windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`, `macos-x64`, `macos-arm64`.
- `xxmi-lang-<version>.vsix`, the VS Code extension.
- `SHA256SUMS`, the SHA-256 of every zip and the vsix.

Each zip holds:

- `xxmi` (`xxmi.exe` on Windows), the CLI.
- `xxmi-lsp` (`xxmi-lsp.exe` on Windows), the language server. It is the same binary as `xxmi`; the name it is started under selects the language server.
- `LICENSE` and `README.txt`.
- An `editors/` folder with `editors/manifest.json`.

## Commands and flags

- `xxmi lint [paths…]` with `--format text|json`, `--rules`, `--max-warnings`, `--require-package`, `--mi-package`, `--spec`, `--stdin` and `--stdin-path`.
  With `--require-package`, files that are not inside a package (no `d3dx.ini` in a parent folder) are skipped. The editor setting `xxmi.requirePackage` does the same.
  `--mi-package ID` sets the game profile of every package; its values are the `id`s in `profiles` below.
- `xxmi query`, `xxmi index`, `xxmi lsp [--stdio]` and `xxmi capabilities [--json]`.
- `xxmi --version` / `-v` and `xxmi --help` / `-h`.

An unknown command or flag is a usage error (exit 2). The full list of flags is in `04-cli.md`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success. |
| 1 | Lint found an error-severity diagnostic, or warnings exceeded `--max-warnings`. `query` found nothing. |
| 2 | Usage error: no arguments (prints usage), unknown command or flag, invalid value, missing path. |
| 3 | Internal error or panic. |

No other exit codes are used.

## Version output

`xxmi --version`, `xxmi -v` and `xxmi-lsp --version` print the bare semver and a newline, for example `0.2.2`. No prefix will be added without a major bump.

## Lint JSON (version 1)

`xxmi lint --format json` prints one document. Its shape is in `schema/lint-output.v1.json`. The units and order are part of the contract:

- Ranges use 0-based lines and UTF-16 code-unit columns, the LSP default. The text format shows both 1-based.
- `files` are sorted byte-wise by the displayed `path`.
- `diagnostics` within a file are sorted by start line, then start character, then rule id.

Version 1 is frozen: a change to any field bumps `version`. Per-rule docs and quick-fix info come from `capabilities --json`, not from the lint output.

## Capabilities JSON

`xxmi capabilities --json` prints one line of JSON:

- `capabilitiesVersion`: `1`. Bumped only when a key changes meaning or goes away.
- `version`: the binary's semver.
- `lintSchemaVersions`: the `version` values that `xxmi lint --format json` can emit, currently `[1]`.
- `profiles`: one object per game profile, with `id`, `name`, `dialect` (`xxmi` or `gimi`) and `hasLibrary`.
- `rules`: one object per rule, in id order, with `id`, `defaultSeverity` (`error`, `warning`, `info` or `hint`), `title`, `docUrl` (an anchor in `02-diagnostics.md`) and `hasFix`.

Without `--json`, it prints one line per rule: `ID  severity  title`.

## Semver policy

The version follows semver. Anything not listed in this page is not part of the contract.

- **Minor:** a new rule id, or a new optional field in the capabilities JSON.
- **Major (breaking):** a removed or renamed flag, a changed exit code, a changed default severity, a changed lint JSON field, or a changed asset name.
