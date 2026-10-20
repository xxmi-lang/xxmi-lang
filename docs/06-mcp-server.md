# 06 — Hosted MCP server (Railway)

A public, read-only MCP server so anyone's agent can query the XXMI DLL, game packages and XXMITools, and can lint ini text, without cloning or updating repos locally.

This page is the original design. **What was built, and where it differs, is in "As built (M6)" at the end.**

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

Other clients (Cursor, Codex, etc.) use the same URL with their HTTP MCP config. No server is deployed yet, so the README does not name a URL; fill it in after the Railway setup below.

## Tests

- Unit: each tool handler against a small fixture repo checked into `packages/mcp-server/test/fixtures/repos/`.
- Integration: start the server against fixture repos, connect with the MCP SDK client, and call every tool.
- Contract: snapshot `tools/list` output, so changes to tool names or schemas are deliberate.

## As built (M6)

Status (2026-10-20): implemented in `packages/mcp-server`, tested, and run from its Docker image locally. **Not deployed:** that needs a Railway account (see "Railway setup"). The sections above stay as the design; this one is what exists.

### Endpoints

| Path | What |
|---|---|
| `POST /mcp` | MCP Streamable HTTP, stateless, JSON responses (no SSE). A fresh MCP server and transport per request. `GET`/`DELETE` answer 405. |
| `GET /healthz` | Always 200 once listening: `{"status":"ok","ready":false,"version":"…"}`. `ready` becomes true when every tracked repo has its default branch indexed. Not rate limited. |
| `POST /admin/refresh` | Runs an index pass now. Needs `Authorization: Bearer $ADMIN_TOKEN`; does not exist (404) if `ADMIN_TOKEN` is unset. Returns 202 at once, or the pass result with `?wait=1`. |
| `GET /` | Name and version. |

### Tools

All tools return compact text, or JSON with `format: "json"`. Errors come back as MCP tool errors with a message that says what to change. Argument errors (wrong types, missing fields) are rejected by the MCP SDK's schema validation.

| Tool | Input | Returns |
|---|---|---|
| `spec_lookup` | `name`, `kind?` (section, key, command, operator, function, member, enum), `section?`, `dll_ref?` | Spec entries: facts, docs, example, XXMI-only flag, the spec's DLL symbol and its line in the indexed DLL source (with permalink). `ResourceFoo` finds the `Resource` section, `->Region` the member. |
| `spec_list` | `kind`, `section?` (needed for `key`), `cursor?` | Names with one-line docs. |
| `library_list` | `game`, `ref?`, `cursor?` | Libraries (namespaces) with symbol counts, files and the header comment of the library. |
| `library_symbol` | `qualified_name`, `game`, `ref?` | Location and permalink; doc comment (plain `;` block above the header, or `;;` tags: `@desc`, `@in`, `@out`, `@since`, `@example`); the file header; comment blocks elsewhere in the package that mention the symbol (where ZZMI documents inputs today); what the section references; other symbols of the same namespace (candidate inputs); the section source. |
| `find_implementation` | `feature`, `ref?`, `limit?` | Ranked DLL locations: the spec symbol's definition, then the literal in that file with the function it sits in, then other mentions; snippets and permalinks. Falls back to a word search for names outside the spec. |
| `search_source` | `repo`, `query`, `path_glob?`, `ref?`, `max_results?` (≤ 50) | ripgrep matches with 2 lines of context. |
| `read_source` | `repo`, `path`, `start_line`, `end_line` (≤ 400 lines), `ref?` | Numbered excerpt and permalink. |
| `lint_ini` | `files` (`{path: content}`), `game` (default `zzmi`), `package_ref?` | `xxmi lint` output: text, or the `version: 1` JSON document (same shape) plus the package commit used. Paths decide namespaces (`Mods/MyMod/mod.ini`) and `include =` works between the submitted files. |
| `list_versions` | — | Indexed repos and refs, the DLL commit the spec was extracted from, ready flag. |

`repo` is `dll`, `zzmi`, `xxmitools` or `owner/name` of a tracked repo. `ref` is a branch, a tag or a commit prefix (7+ characters) of something indexed; omitted or `latest` means the default branch. Resources: `xxmi://spec/{dll_ref}/sections.json` and `…/commands.json` (the merged spec; `dll_ref` is `latest` or the spec's commit).

The logic is in `@xxmi-lang/core` (`src/query/`, `lintTexts`, `VirtualPathResolver`); the server only maps tool arguments to core calls, serves checkouts and enforces limits. `xxmi query` (`04-cli.md`) uses the same functions.

### Environment variables

| Variable | Default | Meaning |
|---|---|---|
| `PORT` | 3000 | Set by Railway. |
| `HOST` | `0.0.0.0` | |
| `DATA_DIR` | `/data` in the image, `./data` otherwise | The volume. |
| `POLL_INTERVAL_MIN` | 15 | Minutes between index passes; 0 turns polling off. |
| `INDEXED_TAGS` | 5 | Newest release tags indexed next to the default branch. |
| `GITHUB_TOKEN` | unset | Optional. Without it GitHub's unauthenticated API allows 60 requests an hour per IP; a pass makes 6 (two per repo; the first one after a start 9, because it also asks for the default branch name), and unchanged answers revalidate with ETags, which GitHub doesn't count; tarballs come from codeload, which isn't part of the API limit. Set it if the shared Railway IP runs out. |
| `ADMIN_TOKEN` | unset | Enables `/admin/refresh`. |
| `RATE_LIMIT_PER_MIN` | 60 | Sustained requests per client IP; 0 disables. |
| `RATE_LIMIT_BURST` | 20 | Bucket size. |
| `TRUSTED_PROXY_HOPS` | 0 (1 in the image) | Proxies in front of the server; the client IP is that many entries from the right of `X-Forwarded-For`, so a client can't spoof it. |
| `MAX_BODY_BYTES` | 3000000 | Request body cap (413). |
| `MAX_LINT_BYTES` / `MAX_LINT_FILES` | 1000000 / 200 | `lint_ini` caps. |
| `TOOL_TIMEOUT_MS` | 15000 | Per tool call. |
| `SEARCH_TIMEOUT_MS` | 2000 | `search_source`: ripgrep is killed after this and partial results are returned. |
| `INDEXER` | on | `off` serves only what the volume holds (no GitHub access). |

### Volume and indexer

```
/data/state.json                       which refs are indexed (atomic rename)
/data/repos/<owner>/<repo>/<sha>/      text files of one commit (tarball extract)
/data/index/<owner>__<repo>/<sha>.json library symbol snapshot of a package commit
/data/tmp/                             staging; cleared at start
```

- Every `POLL_INTERVAL_MIN` the indexer asks GitHub for the default branch head and the newest tags of each tracked repo, downloads the tarball of every commit not on disk (`codeload.github.com`, no `git` or clone needed), extracts only text files (`.ini`, `.cpp`, `.h`, `.hlsl`, …; for the DLL repo only its own folders, not pcre2 and the like), builds the package symbol snapshot with core, then renames the checkout into place and rewrites `state.json`. Readers never see a half-written commit; a failed download leaves the old one served. Old commits are removed one pass after they stop being referenced.
- Default branches are indexed first, then tags, so the server is `ready` after about 10 seconds on a first boot; the tags follow within a minute. A restart reads `state.json` and serves from the volume at once.
- Until the first ZZMI index is ready, `library_*` and `lint_ini` answer from the snapshot bundled with the server (locations only, with a note); spec tools always work. The DLL tools say the server is warming up.
- If GitHub can't be reached before the first index exists, the indexer retries after a minute instead of waiting for the next poll.
- A new ZZMI-Package commit is served within one poll interval plus about 30 seconds, so within 16 minutes by default.

### Safety

- Repos come from a code allowlist (`src/repos.ts`); callers can't name other URLs.
- `read_source` and `search_source` paths are normalized and must stay inside the checkout (absolute paths, drive letters, `..`, NUL are refused); extraction skips symlinks.
- `lint_ini` never touches the server's disk: files live in a virtual file system (`VirtualPathResolver`), so `include = ..\..\etc\hosts` just reports "not found", and `xxmi.toml` is not read. Paths must be relative `.ini` paths without `..`.
- Input caps and timeouts as in the table above; `search_source` uses ripgrep (linear-time regex), so a hostile pattern can't hang the server.
- Rate limit per client IP: token bucket in memory, 429 with `Retry-After`.
- Logs are JSON lines: request method, path, status and timing; tool name and timing; `lint_ini` sizes and counts. Submitted text is never logged. A client IP is logged only when it is rate limited.
- Unauthenticated and read-only; the only write path is the indexer.

### Docker and Railway

`packages/mcp-server/Dockerfile` (build context: the repo root) builds with Node 22 and pnpm (`pnpm deploy --prod`), and runs on plain Alpine with only the `node` binary copied over, plus ripgrep: about 240 MB uncompressed, 37 MB of it the app. It starts as root only to `chown` the volume mount (Railway mounts volumes as root), then runs `node --liftoff-only dist/main.js` as the unprivileged `node` user. The flag avoids the web-tree-sitter memory leak (`packages/core/src/runtime.ts`). The Dockerfile has a `HEALTHCHECK`; Railway uses `railway.toml` instead.

Run it locally:

```bash
docker build -f packages/mcp-server/Dockerfile -t xxmi-mcp .
docker run --rm -p 3000:3000 -v xxmi-data:/data xxmi-mcp
node scripts/mcp-acceptance.ts http://localhost:3000/mcp
```

From source: `pnpm build && DATA_DIR=./data node --liftoff-only packages/mcp-server/dist/main.js`.

**Railway setup (needs the maintainer's account):**

1. New project, "Deploy from GitHub repo", pick `xxmi-lang/xxmi-lang`. `railway.toml` at the repo root selects the Dockerfile, the `/healthz` health check and the restart policy; nothing to configure in the build settings.
2. In the service: add a **volume** and mount it at `/data`.
3. Variables: `ADMIN_TOKEN` (any long random string; optional), `GITHUB_TOKEN` (optional, no scopes needed). Leave `PORT` alone; the image already sets `DATA_DIR=/data` and `TRUSTED_PROXY_HOPS=1`. Keep the service at one replica.
4. Settings → Networking → **Generate domain** (or add a custom domain). In Settings → Deploy, tick "Wait for CI" so a push to `main` deploys only after the checks pass.
5. Check `https://<domain>/healthz` (`ready` turns true after about 10 seconds), then `node scripts/mcp-acceptance.ts https://<domain>/mcp`.
6. Put the URL in the README and the plugin: change `plugin/.mcp.json` to `${XXMI_MCP_URL:-https://<domain>/mcp}` (`07-agent-integration.md`), and the client setup below.

### Deviations from the design above

- **No git, no clones.** Tarballs by commit sha instead of `git ls-remote` plus bare clones and worktrees; the image needs no `git`. Same effect, much smaller volume (a few MB per commit).
- **Default `ref` is the default branch's head, not the latest release**, because the acceptance criterion is that a new package commit is served within 20 minutes. Pass a tag to get a release.
- **Spec is the one bundled with the server**, not re-extracted on DLL changes. `spec_lookup` shows both the spec's DLL commit and the line in the indexed DLL head, which can drift apart until the server is rebuilt (the weekly `spec-extract` workflow opens the PR that updates the spec). Only `dll_ref: latest` or the spec's own commit is served.
- **Tracked repos are code (`src/repos.ts`), not `config/repos.toml`.**
- **No `migration_guide` tool and no `xxmi://eras/…` resources:** the era docs belong to M5, which moved (`09-roadmap.md`).
- **No `XXMI_MCP_KEYS`** API keys, no webhook endpoint.
- **`lint_ini`'s `game` defaults to `zzmi`** (the only game profile so far), and its JSON adds a `package` field (repository and commit used).
- `/admin/refresh` answers 202 and runs in the background unless `?wait=1`.

### Tests

`packages/mcp-server/test`: tools through the SDK's in-memory transport on a fixture package and DLL (with a contract snapshot of `tools/list`), the indexer with a fake GitHub serving fixture tarballs (first pass, no-change pass, atomic swap, failed download, restart from the volume), the HTTP server with the real SDK client (rate limit, spoofed `X-Forwarded-For`, body cap, lint caps, admin token), ripgrep search, path sandboxing, the GitHub client with a mocked `fetch`. `scripts/mcp-acceptance.ts <url>` asks the three acceptance questions of a running server; CI builds the image and starts it on every run, and runs the script against it weekly.
