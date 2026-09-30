# 06 — Hosted MCP server (Railway)

A public, read-only MCP server so anyone's agent can query the XXMI DLL, game packages and XXMITools, and can lint ini text, without cloning or updating repos locally.

## Architecture

```
Railway service "xxmi-mcp" (1 replica, Dockerfile deploy)
├── HTTP server (Node 22)
│   ├── POST/GET /mcp        MCP Streamable HTTP transport (stateless mode)
│   ├── GET /healthz         Railway healthcheck
│   └── POST /admin/refresh  token-protected manual reindex
├── Indexer (in-process scheduler)
│   ├── every 15 min: `git ls-remote` each tracked repo; fetch on change
│   ├── per new ref: build index with @xxmi-lang/core (`xxmi index`) + ripgrep-ready checkout
│   └── on DLL change: run spec extractor → versioned spec
└── Volume mounted at /data
    ├── repos/<owner>/<repo>/            bare clone + worktrees per indexed ref
    ├── index/<repo>/<ref>/symbols.json   core index snapshot
    └── spec/<dll-ref>/…                  generated spec
```

- **One replica.** A Railway volume attaches to a single service instance, and the load is small. Scale vertically if needed.
- **Tracked repos** (allowlist in `mcp-server/config/repos.toml`; the server never clones arbitrary URLs):
  - `SpectrumQT/XXMI-Libs-Package`: `master` plus release tags
  - `leotorrez/ZZMI-Package`: `main` plus tags
  - `leotorrez/XXMITools`: `main` plus tags
  - Later: other game packages (GIMI, SRMI, WWMI, HIMI)
- **Indexed refs:** the default branch plus the last N release tags (N = 5). Tools take an optional `ref`, defaulting to the latest release, so users on older versions get matching answers.
- **Polling, not webhooks,** since we don't control all upstream repos. Add a webhook endpoint later for repos we do control.

## Tools

Keep tool descriptions short, since they cost context in every agent session. Every tool returns compact text by default, or structured JSON when `format: "json"`. Paginate with `cursor`.

| Tool | Input | Returns |
|---|---|---|
| `spec_lookup` | `name`, `kind?` (section\|key\|command\|operator), `dll_ref?` | Spec entry: docs, args, "XXMI-only" flag, example, and the DLL source location (file:line + permalink) |
| `spec_list` | `kind`, `section?` | Names and one-line docs, for discovery (e.g. all keys valid in `[Resource*]`) |
| `library_list` | `game` (zzmi…), `ref?` | Libraries in the package with their namespaces and module docs |
| `library_symbol` | `qualified_name` (e.g. `CommandList\ZZMI\SetTextures`, `$\TTL\alpha`), `game`, `ref?` | Declaration, doc comment, inputs/outputs, usage example, source snippet |
| `find_implementation` | `feature` (e.g. `store`, `Pool`, `->Region`) | Where the DLL implements it: ranked file:line list with short snippets |
| `search_source` | `repo`, `query` (regex), `path_glob?`, `ref?`, `max_results?` (≤ 50) | ripgrep matches with 2 lines of context |
| `read_source` | `repo`, `path`, `start_line`, `end_line` (≤ 400 lines), `ref?` | File excerpt with line numbers |
| `lint_ini` | `files`: `{path: content}` (≤ 1 MB total), `game`, `package_ref?` | Diagnostics in the same format as `xxmi lint --format json`. Resolves library references against the indexed package |
| `migration_guide` | `game`, `from_era?`, `to_era?` | The era docs and recipe step list (auto vs manual), so remote agents can follow a migration by hand |
| `list_versions` | — | Indexed repos and refs, plus the DLL commit the spec came from |

MCP **resources** (read-only): `xxmi://spec/{dll_ref}/sections.json`, `…/commands.json`, `xxmi://eras/{game}/{era}.md`.

## Security and limits

- Read-only. No tool writes to repos or the volume except the indexer.
- Repo allowlist; `path` inputs are normalized and must stay inside the checkout.
- Input caps: `lint_ini` ≤ 1 MB total, ≤ 200 files; `search_source` regex timeout 2 s (ripgrep `--max-count`, plus a wall-clock kill).
- Rate limit per IP (token bucket, e.g. 60 req/min) in memory. Optional API keys (env `XXMI_MCP_KEYS`) for higher limits later.
- **Privacy:** don't persist or log the content submitted to `lint_ini`. Log only sizes, timings and rule counts.
- `/admin/refresh` requires `Authorization: Bearer $ADMIN_TOKEN`.

## Deployment

```dockerfile
FROM node:22-bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git ripgrep ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN corepack enable && pnpm install --frozen-lockfile && pnpm --filter @xxmi-lang/mcp-server... build
ENV NODE_ENV=production DATA_DIR=/data
CMD ["node", "packages/mcp-server/dist/main.js"]
```

`railway.toml`:

```toml
[build]
builder = "DOCKERFILE"
dockerfilePath = "packages/mcp-server/Dockerfile"

[deploy]
healthcheckPath = "/healthz"
restartPolicyType = "ON_FAILURE"
```

- Env: `PORT` (set by Railway), `DATA_DIR=/data`, `ADMIN_TOKEN`, `POLL_INTERVAL_MIN=15`, `INDEXED_TAGS=5`.
- Attach a volume at `/data`. On first boot the indexer clones everything. `/healthz` returns 200 immediately and a `"ready"` flag once the first index finishes; tools return a "warming up" error until then.
- Custom domain optional (e.g. `mcp.xxmi.dev`).
- Deploy on push to `main` via Railway's GitHub integration. CI runs tests first.

## Client setup (for the README)

```bash
claude mcp add --transport http xxmi https://<your-domain>/mcp
```

Other clients (Cursor, Codex, etc.) use the same URL with their HTTP MCP config.

## Tests

- Unit: each tool handler against a small fixture repo checked into `packages/mcp-server/test/fixtures/repos/`.
- Integration: start the server against fixture repos, connect with the MCP SDK client, and call every tool.
- Contract: snapshot `tools/list` output, so changes to tool names or schemas are deliberate.
