# 06 — Hosted MCP server (Railway)

A public, read-only MCP server so anyone's agent can query the XXMI DLL, game packages and XXMITools, and can lint ini text, without cloning or updating repos locally.

This page is the original design. **What was built, and where it differs, is in "As built (M6)" at the end.**

## Architecture

```
Railway service "xxmi-mcp" (1 replica, Dockerfile deploy)
├── HTTP server (Rust, axum)
│   ├── POST/GET /mcp        MCP Streamable HTTP transport (stateless mode)
│   ├── GET /healthz         Railway healthcheck
│   └── POST /admin/refresh  token-protected manual reindex
├── Indexer (in-process scheduler)
│   ├── every 15 min: `git ls-remote` each tracked repo; fetch on change
│   ├── per new ref: build index with `xxmi-core` (`xxmi index`) + ripgrep-ready checkout
│   └── on DLL change: run spec extractor → versioned spec
└── Volume mounted at /data
    ├── repos/<owner>/<repo>/            bare clone + worktrees per indexed ref
    ├── index/<repo>/<ref>/symbols.json   core index snapshot
    └── spec/<dll-ref>/…                  generated spec
```

- **One replica.** A Railway volume attaches to a single service instance, and the load is small. Scale vertically if needed.
- **Tracked repos** (allowlist in `rust/crates/xxmi-mcp/src/repos.rs`; the server never clones arbitrary URLs):
  - `SpectrumQT/XXMI-Libs-Package`: `master` plus release tags
  - `leotorrez/ZZMI-Package`: `main` plus tags
  - `leotorrez/XXMITools`: `main` plus tags
  - Later: other game packages (GIMI, SRMI, WWMI, HIMI)
- **Indexed refs:** the default branch plus the last N release tags (N = `INDEXED_RELEASES`, default 3). Tools take an optional `ref`, defaulting to the latest release, so users on older versions get matching answers.
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

Railway autodeploys pushes to `main` once CI passes. Build settings live in the Railway service, not in the repo:

- Dockerfile: `docker/mcp/Dockerfile` (`dockerfilePath`)
- Health check: `/healthz`, 60 s timeout (`healthcheckPath`, `healthcheckTimeout`)
- Restart policy: `ON_FAILURE`, up to 10 retries (`restartPolicyType`, `restartPolicyMaxRetries`)

- Env: `PORT` (set by Railway), `DATA_DIR=/data`, `ADMIN_TOKEN`, `POLL_INTERVAL_MIN=15`.
- `RAILWAY_DOCKERFILE_PATH=docker/mcp/Dockerfile`: the service needs this to build the Dockerfile above.
- Attach a volume at `/data`. On first boot the indexer clones everything. `/healthz` returns 200 immediately and a `"ready"` flag once the first index finishes; tools return a "warming up" error until then.
- Custom domain optional (e.g. `mcp.xxmi.dev`).

## Client setup (for the README)

```bash
claude mcp add --transport http xxmi https://<your-domain>/mcp
```

Other clients (Cursor, Codex, etc.) use the same URL with their HTTP MCP config. The public instance is `https://xxmi-mcp-production.up.railway.app/mcp` (Railway default domain; a custom domain later).

## Tests

- Unit: each tool handler against a small fixture repo checked into `rust/crates/xxmi-mcp/tests/fixtures/`.
- Integration: start the server against fixture repos, connect with the MCP SDK client, and call every tool.
- Contract: snapshot `tools/list` output, so changes to tool names or schemas are deliberate.

## As built (M6)

Status (2026-10-20): implemented in `rust/crates/xxmi-mcp` (ported from the former TypeScript server, now removed), tested, and **deployed** to Railway as service `xxmi-mcp` at `https://xxmi-mcp-production.up.railway.app/mcp` (default domain; a custom domain later), with a volume at `/data`, one replica, autodeployed from `main` by Railway after CI passes (see Deployment). The sections above stay as the design; this one is what exists.

### Endpoints

| Path | What |
|---|---|
| `POST /mcp` | MCP Streamable HTTP, stateless, JSON responses (no SSE). A fresh MCP server and transport per request. `GET`/`DELETE` answer 405. |
| `GET /healthz` | Always 200 once listening: `{"status":"ok","ready":false,"version":"…"}`. `ready` becomes true when every tracked repo except the baseline (bo3b/3Dmigoto) has its default branch indexed; the baseline's own state is the `indexed` flag of its entry in `list_versions`. Not rate limited. |
| `POST /admin/refresh` | Runs an index pass now, and asks GitHub about releases again even if the tags didn't change. Needs `Authorization: Bearer $ADMIN_TOKEN`; does not exist (404) if `ADMIN_TOKEN` is unset. Returns 202 at once, or the pass result with `?wait=1`. |
| `GET /` | Name and version. |

### Tools

All tools return compact text, or JSON with `format: "json"`. Errors come back as MCP tool errors with a message that says what to change. Argument errors (wrong types, missing fields) are rejected by the MCP SDK's schema validation.

| Tool | Input | Returns |
|---|---|---|
| `spec_lookup` | `name`, `kind?` (section, key, command, operator, function, member, enum), `section?`, `target?` (resource or shader), `dll_ref?` | Spec entries: facts, docs, example, XXMI-only flag, the spec's DLL symbol and its line in the indexed DLL source (with permalink). `ResourceFoo` finds the `Resource` section, `->Region` the member. `target` limits member lookups to resource members (e.g. `ResourceX->Region`) or shader members (e.g. `ps->cb_size`). `dll_ref` picks the DLL version, see "Specs per DLL version". |
| `spec_list` | `kind`, `section?` (needed for `key`), `cursor?`, `dll_ref?` | Names with one-line docs. Members are tagged `[resource]` or `[shader]`; commands written as a value (e.g. `run = snapshotinitialtextures`) are tagged `[value form]`. |
| `library_list` | `game`, `ref?`, `cursor?` | Libraries (namespaces) with symbol counts, files and the header comment of the library. |
| `library_symbol` | `qualified_name`, `game`, `ref?` | Location and permalink; doc comment (plain `;` block above the header, or `;;` tags: `@desc`, `@in`, `@out`, `@since`, `@example`); the file header; comment blocks elsewhere in the package that mention the symbol (where ZZMI documents inputs today); what the section references; other symbols of the same namespace (candidate inputs); the section source. |
| `find_implementation` | `feature`, `ref?`, `limit?` | Ranked DLL locations: the spec symbol's definition, then the literal in that file with the function it sits in, then other mentions; snippets and permalinks. Falls back to a word search for names outside the spec. |
| `search_source` | `repo`, `query`, `path_glob?`, `ref?`, `max_results?` (≤ 50) | ripgrep matches with 2 lines of context. |
| `read_source` | `repo`, `path`, `start_line`, `end_line` (≤ 400 lines), `ref?` | Numbered excerpt and permalink. |
| `lint_ini` | `files` (`{path: content}`), `game` (default `zzmi`), `package_ref?`, `dll_ref?` | `xxmi lint` output: text, or the `version: 1` JSON document (same shape) plus the package commit used. Paths decide namespaces (`Mods/MyMod/mod.ini`) and `include =` works between the submitted files. |
| `list_versions` | — | Indexed repos (each with its `role` and `indexed` flag) and refs (each with a `kind`), the latest release and pre-release per repo, the spec status of each DLL version, the spec used by default, the DLL commit the bundled spec was extracted from, ready flag. See "Versions" and "Specs per DLL version". |

`repo` is `dll`, `zzmi`, `xxmitools` or `owner/name` of a tracked repo. `ref` is a branch, a tag or a commit prefix (7+ characters) of something indexed, or one of `latest` / `head` (the default branch's head, also what an omitted `ref` means), `latest-release`, `latest-prerelease`. The baseline repo (`migoto`, bo3b/3Dmigoto) is head-only and not offered by the package or library tools. Resources: `xxmi://spec` (the default spec), `xxmi://spec/{dll_ref}`, and `xxmi://spec/{dll_ref}/sections.json` / `…/commands.json`, all resolved like the tools' `dll_ref`; the JSON carries a `_spec` block (see "Specs per DLL version").

The logic is in `xxmi-core` (`rust/crates/xxmi-core/src/query/`, `lint_texts`); the server only maps tool arguments to core calls, serves checkouts and enforces limits. `xxmi query` (`04-cli.md`) uses the same functions.

### Environment variables

| Variable | Default | Meaning |
|---|---|---|
| `PORT` | 3000 | Set by Railway. |
| `HOST` | `0.0.0.0` | |
| `DATA_DIR` | `/data` in the image, `./data` otherwise | The volume. |
| `POLL_INTERVAL_MIN` | 15 | Minutes between index passes; 0 turns polling off. |
| `INDEXED_RELEASES` | 3 | Newest GitHub releases indexed next to the default branch. Repos without releases get their newest tags instead. |
| `INDEXED_PRERELEASES` | 2 | Newest pre-releases indexed, only those newer than the newest release. |
| `INDEXED_TAGS` | unset | Deprecated: sets `INDEXED_RELEASES` when that is absent (it used to default to 5). |
| `GIT_CACHE_MAX_MB` | 512 | A repo's bare git cache above this size is deleted after pruning and rebuilt by the next fetch. |
| `GITHUB_TOKEN` | unset | Optional, no longer needed. Git access to the public repos is anonymous. It is sent only with the one remaining GitHub API call (release metadata, see "Versions"), whose unauthenticated limit of 60 requests an hour per IP is far above what that call needs. |
| `ADMIN_TOKEN` | unset | Enables `/admin/refresh`. |
| `RATE_LIMIT_PER_MIN` | 60 | Sustained requests per client IP; 0 disables. |
| `RATE_LIMIT_BURST` | 20 | Bucket size. |
| `TRUSTED_PROXY_HOPS` | 0 (1 in the image) | Proxies in front of the server; the client IP is that many entries from the right of `X-Forwarded-For`, so a client can't spoof it. |
| `MAX_BODY_BYTES` | 3000000 | Request body cap (413). |
| `MAX_LINT_BYTES` / `MAX_LINT_FILES` | 1000000 / 200 | `lint_ini` caps. |
| `TOOL_TIMEOUT_MS` | 15000 | Per tool call. |
| `SEARCH_TIMEOUT_MS` | 2000 | `search_source`: ripgrep is killed after this and partial results are returned. |
| `INDEXER` | on | `off` serves only what the volume holds (no network access). |

### Volume and indexer

```
/data/state.json                       which refs are indexed, and the release metadata (atomic rename)
/data/git/<owner>/<repo>.git           bare repo holding only shallow fetches of indexed commits
/data/repos/<owner>/<repo>/<sha>/      text files of one commit (git archive extract)
/data/index/<owner>__<repo>/<sha>.json library symbol snapshot of a package commit
/data/specs/<dllSha>.json              spec built from one DLL commit, with its build header
/data/tmp/                             staging; cleared at start
```

- Sources come from plain git, not the GitHub API. Every `POLL_INTERVAL_MIN` the indexer runs one `git ls-remote --symref` per tracked repo (default branch, its head, all tags; annotated tags use their peeled commit). For each wanted commit not on disk it does `git fetch --depth 1 --no-tags origin <sha>` into the repo's bare clone (falling back to the ref that pointed at the sha), then `git archive` streams the tree into the extractor, which keeps only text files (`.ini`, `.cpp`, `.h`, `.hlsl`, …; for the DLL repo only its own folders, not pcre2 and the like; for 3Dmigoto only what the spec extractor reads: `DirectX11/`, `D3DCompiler/`, `Injector/` and the root `util.h`). There is never a full clone. The server builds the package symbol snapshot with core, then renames the checkout into place and rewrites `state.json`. Readers never see a half-written commit; a failed fetch leaves the old one served. Old commits are removed one pass after they stop being referenced.
- Timeouts: 30 s for `ls-remote`, 180 s for fetch and archive. Git runs without a shell, with `GIT_TERMINAL_PROMPT=0` and protocol v2, and refs and shas follow `--end-of-options`. A cache that fails its health check (not a bare repo, wrong `origin`), or on which fetch or archive fails with a git error other than a timeout (a stale `shallow.lock`, corruption), is deleted, rebuilt and retried once, at most once per repo per poll pass; one `git cache re-created` line is logged. The bare clones only grow, so after pruning a repo whose `git/` folder exceeds `GIT_CACHE_MAX_MB` is deleted and fetched afresh when next needed.
- Default branches are indexed first, then tags and releases, so the server is `ready` after about 10 seconds on a first boot (a baseline that fails to index does not hold this back); the tags follow within a minute. A restart reads `state.json` and serves from the volume at once.
- Until the first ZZMI index is ready, `library_*` and `lint_ini` answer from the snapshot bundled with the server (locations only, with a note); spec tools always work. The DLL tools say the server is warming up.
- If git can't reach GitHub before the first index exists, the indexer retries after a minute instead of waiting for the next poll.
- A new ZZMI-Package commit is served within one poll interval plus about 30 seconds, so within 16 minutes by default.

### Versions

Git can't tell a release from a pre-release, so the one GitHub API call left is `GET https://api.github.com/repos/{owner}/{repo}/releases?per_page=30`. It runs only when a repo's set of tags differs from the one stored at the last successful lookup, and once at the first start; `/admin/refresh` forces it. The answer (`tag_name` to pre-release flag, draft flag, publish date, name) is kept in `state.json` until the tags change again. If the call fails or is rate limited, the previous map stays, new tags are indexed as plain tags (kind `tag`) until it works, one warning is logged, and the call is retried on a later poll at most once an hour.

Each indexed version has a `kind`: `head` (default branch), `release`, `prerelease`, or `tag` (a git tag without a GitHub release, or not yet classified), and for tags its `tag` name and `publishedAt` when known. Per repo the indexer keeps the head, the newest `INDEXED_RELEASES` releases, and the newest `INDEXED_PRERELEASES` pre-releases that are newer than the newest release, ordered by `publishedAt` (tag-name order when dates are missing). Drafts are ignored. A repo with no releases at all keeps its newest `INDEXED_RELEASES` tags. The `baseline` role (bo3b/3Dmigoto) keeps the head only and never queries releases. The policy per role is in `rust/crates/xxmi-mcp/src/selection.rs`.

`list_versions` adds `latestRelease` and `latestPrerelease` (`{name, sha, publishedAt}` or null) per repo and a `kind` on every version; the text output has a line each, e.g. `latest release: v1.2.3 (2026-10-31)`. No tool's default `ref` changed: it is still the default branch's head.

### Specs per DLL version

The server extracts a spec from every indexed DLL version (head, releases, pre-releases) with the in-process spec extractor (`xxmi-spec-extractor`), using the baseline (bo3b/3Dmigoto head) for the xxmi flags. The overlay (`spec/overlay/*.toml`) is the one shipped with the server build, merged the same way core merges it into the bundled spec; it is not fetched at runtime. A new DLL release is therefore served without a redeploy, and the daily `spec-extract` PR only refreshes the fallback.

- **When:** at the end of an index pass, after the new state is published, so builds never delay `ready`. One build at a time. If the baseline is not indexed yet, nothing is built and the next pass tries again. A failing build is recorded and never fails the pass.
- **Where:** builds run on a blocking thread (`rust/crates/xxmi-mcp/src/spec_build.rs`). After 120 s a build is recorded as failed, though its thread runs until it ends. One extraction takes about 250 ms of synchronous CPU, which would stall every HTTP request on the async workers.
- **Volume:** `/data/specs/<dllSha>.json` holds `{dllSha, baselineSha, builder, builtAt, status, error?, warnings, spec}`, written by tmp and rename. An entry is used only when its `builder` (server version, extractor version and overlay hash) and `baselineSha` match the running ones; otherwise it is rebuilt. A `failed` entry is not retried until one of those changes. Specs of versions that are pruned are removed with them; only indexed versions' specs are held in memory.
- **`dll_ref`** (`spec_lookup`, `spec_list`, `lint_ini`, the spec resources): `latest-release` (default), `latest-prerelease`, `head` / `latest`, a tag name, a commit prefix (7+ characters) or `bundled`. An unknown ref is an error that lists the valid choices.
- **Fallback:** if the version's spec is pending or failed, or nothing is indexed yet, the bundled spec answers and the result's `spec.note` says which version was requested, why, and which DLL commit the bundled spec is from.
- **Reporting:** every result has `spec: {source, dllRef, dllSha, tag, kind, dllCommitDate, note?}` (`source` is `extracted` or `bundled`) in the JSON, and the text ends with one line such as `spec: XXMI-Libs-Package v1.2.3 (release, 065dc74)` or `spec: bundled (b0bff3f); requested v1.3.1 (b0bff3f) is still building`.
- **`list_versions`:** each DLL version has `spec: "ok" | "pending" | "failed"` (and `specError`), and the top level has `defaultSpec: {source, dllSha, tag, kind}`; the existing `spec` object still describes the bundled spec.
- Package indexes (`library_*`, `lint_ini` package symbols) are parsed with the bundled spec, so they do not change with the DLL versions indexed.

### Safety

- Repos come from a code allowlist (`rust/crates/xxmi-mcp/src/repos.rs`); callers can't name other URLs.
- `read_source` and `search_source` paths are normalized and must stay inside the checkout (absolute paths, drive letters, `..`, NUL are refused); extraction skips symlinks.
- `lint_ini` never touches the server's disk: files live in a virtual file system, so `include = ..\..\etc\hosts` just reports "not found", and `xxmi.toml` is not read. Paths must be relative `.ini` paths without `..`.
- Input caps and timeouts as in the table above; `search_source` uses ripgrep (linear-time regex), so a hostile pattern can't hang the server.
- Rate limit per client IP: token bucket in memory, 429 with `Retry-After`.
- Logs are JSON lines: request method, path, status and timing; tool name and timing; `lint_ini` sizes and counts. Submitted text is never logged. A client IP is logged only when it is rate limited.
- Unauthenticated and read-only; the only write path is the indexer.

### Docker and Railway

`docker/mcp/Dockerfile` (build context: the repo root) builds `xxmi-mcp` with Rust, then runs it on `debian:trixie-slim` with git, ripgrep and curl (about 311 MB). The container starts as root only to fix the volume's ownership, then drops to the unprivileged `node` user through `docker/mcp/docker-entrypoint.sh`. The Dockerfile has a `HEALTHCHECK`; Railway uses the service's health check setting instead (see Deployment).

Run it locally:

```bash
docker build -f docker/mcp/Dockerfile -t xxmi-mcp .
docker run --rm -p 3000:3000 -v xxmi-data:/data xxmi-mcp
node scripts/mcp-acceptance.ts http://localhost:3000/mcp
```

From source: `cd rust && DATA_DIR=./data cargo run --release -p xxmi-mcp`.

**Railway setup (needs the maintainer's account):**

1. New project, "Deploy from GitHub repo", pick `xxmi-lang/xxmi-lang`. Set the build settings in the service (see Deployment): Dockerfile `docker/mcp/Dockerfile`, health check `/healthz`, restart policy `ON_FAILURE`.
2. In the service: add a **volume** and mount it at `/data`.
3. Variables: `ADMIN_TOKEN` (any long random string; optional), `GITHUB_TOKEN` (optional and normally unnecessary). Leave `PORT` alone; the image already sets `DATA_DIR=/data`. **Set `TRUSTED_PROXY_HOPS=2`** on the service, overriding the image's 1 (the code default is 0): Railway's edge puts two entries at the end of `X-Forwarded-For` (the client, then its own proxy), so with 1 every request is keyed on a different proxy address and the rate limit never triggers (verified on the live service: 60 parallel requests all succeeded with 1; with 2, 20 passed and 40 got 429, also with spoofed `X-Forwarded-For`). Keep the service at one replica.
4. Settings → Networking → **Generate domain** (or add a custom domain). Autodeploy from `main` after CI passes (see Deployment).
5. Check `https://<domain>/healthz` (`ready` turns true after about 10 seconds), then `node scripts/mcp-acceptance.ts https://<domain>/mcp`.
6. (Done.) Put the URL in the README and the plugin: change `plugin/.mcp.json` to `${XXMI_MCP_URL:-https://<domain>/mcp}` (`07-agent-integration.md`), and the client setup below.

### Rust server

`rust/crates/xxmi-mcp` is the hosted server, ported from the former TypeScript one (same tools, endpoints, environment variables and volume layout). Run it from source with `cd rust && cargo run --release -p xxmi-mcp`, or build the image with `docker build -f docker/mcp/Dockerfile -t xxmi-mcp .`. It needs `git` and `rg` on the path.
Railway builds this image (`docker/mcp/Dockerfile`, see Deployment).

Load (`rust/bench/mcp-load.mjs 10 32`): about 5,500 requests/s, p50 2.2 ms, RSS 6 MB idle and 34-39 MB at peak. The former TypeScript server managed about 600 requests/s at 119 MB.

The server also reads these:

- `WORKER_THREADS`: async worker threads. Default: available parallelism, capped at 4.
- `MAX_INFLIGHT`: requests beyond this many in flight get 503 with `Retry-After`. Default: 512.
- `PACKAGE_CACHE_ENTRIES`: maximum number of cached package indexes (LRU). Default: 64.

Tool calls run on blocking threads from a fixed pool of 8, which is not configurable.

### Deviations from the design above

- **Git without clones or worktrees.** `git ls-remote` plus shallow fetches by sha into one bare repo per tracked repo, and `git archive` for the files, instead of full bare clones with worktrees. The volume holds one commit's objects plus its text files, a few MB per commit.
- **Default `ref` is the default branch's head, not the latest release**, because the acceptance criterion is that a new package commit is served within 20 minutes. Pass a tag to get a release.
- **Specs are built per indexed DLL version at runtime** (see "Specs per DLL version"), so a new DLL release needs no redeploy. The spec bundled with the server (updated by the daily `spec-extract` workflow's PR) is the fallback while a version's own spec is missing.
- **Tracked repos are code (`rust/crates/xxmi-mcp/src/repos.rs`), not `config/repos.toml`.**
- **No `migration_guide` tool and no `xxmi://eras/…` resources:** the era docs belong to M5, which moved (`09-roadmap.md`).
- **No `XXMI_MCP_KEYS`** API keys, no webhook endpoint.
- **`lint_ini`'s `game` defaults to `zzmi`** (the only game profile so far), and its JSON adds a `package` field (repository and commit used).
- `/admin/refresh` answers 202 and runs in the background unless `?wait=1`.

### Tests

`rust/crates/xxmi-mcp/tests` (`cargo test --release`): `http.rs`, `limits.rs`, `indexer.rs`, `tools.rs` and `spec_build_tags.rs`, with shared helpers in `tests/support`. `scripts/mcp-acceptance.ts <url>` asks the three acceptance questions of a running server, plus a `dll_ref=head` spec lookup; CI builds the image and starts it on every run, and runs the script against it weekly.
