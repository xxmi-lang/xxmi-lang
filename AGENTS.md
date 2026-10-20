# AGENTS.md

Guidance for AI coding agents (Codex, Cursor, Claude Code and others) working on XXMI /
3DMigoto mod `.ini` files, or on this repository. `CLAUDE.md` has the full project rules.

## Working on mod `.ini` files

Use the tools instead of guessing the language. XXMI added features over vanilla 3DMigoto
(`store`, pools, `->Region(…)`, bitwise operators, `locked` globals) and dropped others.

| Need                                                   | Tool                                                                                 |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| Check a file or folder                                 | `xxmi lint <path>` (exit 1 means errors; `--format json` for data)                   |
| Check text without a file                              | `xxmi lint --stdin --stdin-path <path>`                                              |
| Diagnostics after every edit, go to definition, rename | `xxmi-lsp` (stdio language server, `xxmi lsp --stdio`)                               |
| What a section, key, command or operator means         | `xxmi query command <name>`, `xxmi query keys <section>`, `xxmi query operator <op>` |
| What a ZZMI library symbol takes and does              | `xxmi query symbol 'CommandList\ZZMI\SetTextures'`, `xxmi query library`             |
| No shell, or no local clone of the DLL or package      | the hosted MCP server (below)                                                        |

Fix every `XM0xx`/`XM1xx`/`XM2xx` error before moving on. Don't silence a rule with
`; xxmi-disable-next-line` unless the user agrees, and say why in a comment.

Releases of `xxmi` and `xxmi-lsp` are at https://github.com/xxmi-lang/xxmi-lang/releases;
put them on `PATH`.

### Hosted MCP server

A read-only MCP server over Streamable HTTP (no auth): `claude mcp add --transport http xxmi <url>/mcp`;
other clients take the same URL in their HTTP MCP config. (The public URL is not published
yet; see `docs/06-mcp-server.md`, "As built".) Tools:

- `spec_lookup`, `spec_list`: sections, keys, commands, operators from the DLL spec, with the
  DLL source location.
- `library_list`, `library_symbol`: libraries of a game package (ZZMI) and one symbol's docs,
  usage, references and source.
- `find_implementation`, `search_source`, `read_source`: where the DLL implements a feature,
  and ripgrep and excerpts over the indexed DLL, package and XXMITools sources.
- `lint_ini`: lint ini text passed in the call (`{path: content}`; the path decides the
  namespace) against the indexed package.
- `list_versions`: indexed repos and refs.

Prefer these over reading whole repositories.

## Working on this repository

- Read `CLAUDE.md`, then the doc for the area you touch (`docs/00-overview.md` first).
- Ground truth, in order: the XXMI DLL source, the game package, our tree-sitter-migoto fork,
  then the docs. Never guess the language: every keyword in `spec/` traces to a DLL source
  location or is marked `"source": "manual"` with a note.
- One core: LSP, CLI and MCP call `@xxmi-lang/core`; no parsing, resolution or rule logic in a
  frontend.
- Tests for everything (`pnpm test`), and `pnpm lint && pnpm typecheck` clean. Rules need a
  good and a bad fixture. Don't vendor real mods; fixtures are minimal and hand-written.
- Update the relevant `docs/` file when user-visible behavior changes.
