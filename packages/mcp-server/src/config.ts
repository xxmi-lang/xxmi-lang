/** Server settings from environment variables (docs/06-mcp-server.md "As built"). */

export interface ServerConfig {
  port: number;
  host: string;
  dataDir: string;
  /** Bearer token for `POST /admin/refresh`; the endpoint is disabled without one. */
  adminToken?: string;
  githubToken?: string;
  /** Minutes between checks for new commits; 0 disables polling (tests). */
  pollIntervalMin: number;
  indexedTags: number;
  /** Requests per minute per client IP; 0 disables the limit. */
  rateLimitPerMin: number;
  rateLimitBurst: number;
  /** Proxies in front of the server whose `X-Forwarded-For` entry is trusted. */
  trustedProxyHops: number;
  maxBodyBytes: number;
  maxLintBytes: number;
  maxLintFiles: number;
  toolTimeoutMs: number;
  searchTimeoutMs: number;
  /** Run the indexer at all (off in tests and when only the bundled data is wanted). */
  indexerEnabled: boolean;
}

type Env = Record<string, string | undefined>;

function int(env: Env, name: string, fallback: number, min = 0): number {
  const raw = env[name];
  if (raw === undefined || raw.trim() === '') return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < min) {
    throw new Error(`${name} must be an integer >= ${min}, got "${raw}"`);
  }
  return value;
}

export function loadConfig(env: Env = process.env): ServerConfig {
  const config: ServerConfig = {
    port: int(env, 'PORT', 3000, 1),
    host: env['HOST'] ?? '0.0.0.0',
    dataDir: env['DATA_DIR'] ?? './data',
    pollIntervalMin: int(env, 'POLL_INTERVAL_MIN', 15),
    indexedTags: int(env, 'INDEXED_TAGS', 5),
    rateLimitPerMin: int(env, 'RATE_LIMIT_PER_MIN', 60),
    rateLimitBurst: int(env, 'RATE_LIMIT_BURST', 20, 1),
    trustedProxyHops: int(env, 'TRUSTED_PROXY_HOPS', 0),
    maxBodyBytes: int(env, 'MAX_BODY_BYTES', 3_000_000, 1024),
    maxLintBytes: int(env, 'MAX_LINT_BYTES', 1_000_000, 1),
    maxLintFiles: int(env, 'MAX_LINT_FILES', 200, 1),
    toolTimeoutMs: int(env, 'TOOL_TIMEOUT_MS', 15_000, 100),
    searchTimeoutMs: int(env, 'SEARCH_TIMEOUT_MS', 2_000, 100),
    indexerEnabled: env['INDEXER'] !== 'off',
  };
  if (env['ADMIN_TOKEN']) config.adminToken = env['ADMIN_TOKEN'];
  if (env['GITHUB_TOKEN']) config.githubToken = env['GITHUB_TOKEN'];
  return config;
}
