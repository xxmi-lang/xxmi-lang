/**
 * The HTTP surface: `/mcp` (Streamable HTTP, stateless), `/healthz` and `/admin/refresh`.
 * Stateless means every POST gets a fresh MCP server and transport, so there is no session to
 * pin, leak or lose on a restart, and one replica needs no shared state.
 */
import { timingSafeEqual } from 'node:crypto';
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js';
import { VERSION } from '@xxmi-lang/core';
import type { Catalog } from './catalog.ts';
import type { ServerConfig } from './config.ts';
import type { Indexer } from './indexer.ts';
import { errorMessage, type Logger } from './logger.ts';
import type { RateLimiter } from './ratelimit.ts';
import { createMcpServer } from './tools/index.ts';
import type { ToolContext } from './tools/shared.ts';

export interface HttpDeps {
  config: ServerConfig;
  catalog: Catalog;
  indexer: Indexer;
  limiter: RateLimiter;
  logger: Logger;
  tools: ToolContext;
}

class BodyTooLarge extends Error {}
class BadJson extends Error {}

function sendJson(
  res: ServerResponse,
  status: number,
  body: unknown,
  headers: Record<string, string> = {},
): void {
  if (res.headersSent) return;
  const text = JSON.stringify(body);
  res.writeHead(status, {
    'content-type': 'application/json',
    'content-length': Buffer.byteLength(text),
    ...headers,
  });
  res.end(text);
}

const rpcError = (code: number, message: string): object => ({
  jsonrpc: '2.0',
  error: { code, message },
  id: null,
});

/** The address the rate limit is keyed on: the socket, or `X-Forwarded-For` behind a proxy. */
export function clientIp(req: IncomingMessage, trustedHops: number): string {
  const socketIp = req.socket.remoteAddress ?? 'unknown';
  if (trustedHops <= 0) return socketIp;
  const forwarded = req.headers['x-forwarded-for'];
  const list = (Array.isArray(forwarded) ? forwarded.join(',') : (forwarded ?? ''))
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  // The entry appended by the outermost trusted proxy is the real client; anything to its
  // left is client-supplied and could be spoofed.
  return list.at(-trustedHops) ?? socketIp;
}

async function readJsonBody(req: IncomingMessage, limit: number): Promise<unknown> {
  const declared = Number(req.headers['content-length']);
  if (Number.isFinite(declared) && declared > limit) throw new BodyTooLarge();
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    size += (chunk as Buffer).length;
    if (size > limit) throw new BodyTooLarge();
    chunks.push(chunk as Buffer);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch {
    throw new BadJson();
  }
}

function tokenMatches(header: string | undefined, token: string): boolean {
  const given = Buffer.from(header?.replace(/^Bearer\s+/i, '') ?? '');
  const wanted = Buffer.from(token);
  return given.length === wanted.length && timingSafeEqual(given, wanted);
}

/**
 * Stateless: no session id. The SDK's types don't accept the `undefined` generator its own docs
 * prescribe once `exactOptionalPropertyTypes` is on, hence the casts.
 */
function newStatelessTransport(): StreamableHTTPServerTransport {
  const options = { sessionIdGenerator: undefined, enableJsonResponse: true };
  return new StreamableHTTPServerTransport(
    options as unknown as ConstructorParameters<typeof StreamableHTTPServerTransport>[0],
  );
}

async function handleMcp(deps: HttpDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  if (req.method !== 'POST') {
    sendJson(
      res,
      405,
      rpcError(-32000, 'Method not allowed: this server is stateless, use POST.'),
      {
        allow: 'POST',
      },
    );
    return;
  }
  let body: unknown;
  try {
    body = await readJsonBody(req, deps.config.maxBodyBytes);
  } catch (error) {
    if (error instanceof BodyTooLarge) {
      sendJson(
        res,
        413,
        rpcError(-32600, `Request body exceeds ${deps.config.maxBodyBytes} bytes.`),
      );
      req.destroy();
    } else sendJson(res, 400, rpcError(-32700, 'Parse error: body is not valid JSON.'));
    return;
  }

  const server = createMcpServer(deps.tools);
  const transport = newStatelessTransport();
  res.on('close', () => {
    void transport.close();
    void server.close();
  });
  await server.connect(transport as Transport);
  await transport.handleRequest(req, res, body);
}

async function handleAdmin(
  deps: HttpDeps,
  req: IncomingMessage,
  res: ServerResponse,
  url: URL,
): Promise<void> {
  const token = deps.config.adminToken;
  if (!token) {
    sendJson(res, 404, { error: 'not found' });
  } else if (req.method !== 'POST') {
    sendJson(res, 405, { error: 'use POST' }, { allow: 'POST' });
  } else if (!tokenMatches(req.headers.authorization, token)) {
    sendJson(res, 401, { error: 'invalid or missing bearer token' });
  } else {
    const pass = deps.indexer.runOnce();
    if (url.searchParams.get('wait') === '1') {
      sendJson(res, 200, await pass);
    } else {
      void pass.catch(() => undefined);
      sendJson(res, 202, { status: 'started' });
    }
  }
}

/** Rate-limits the caller; answers 429 itself and returns false when the budget is spent. */
function admit(deps: HttpDeps, req: IncomingMessage, res: ServerResponse, path: string): boolean {
  const ip = clientIp(req, deps.config.trustedProxyHops);
  const decision = deps.limiter.take(ip);
  if (decision.allowed) return true;
  deps.logger.warn('rate limited', { ip, path, retryAfterSec: decision.retryAfterSec });
  sendJson(
    res,
    429,
    { error: 'rate limit exceeded', retryAfterSec: decision.retryAfterSec },
    { 'retry-after': String(decision.retryAfterSec) },
  );
  return false;
}

async function route(deps: HttpDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const { pathname } = new URL(req.url ?? '/', 'http://localhost');
  if (pathname === '/healthz') {
    sendJson(res, 200, { status: 'ok', ready: deps.catalog.ready, version: VERSION });
  } else if (pathname === '/') {
    sendJson(res, 200, { name: 'xxmi-mcp', version: VERSION, mcp: '/mcp', health: '/healthz' });
  } else if (pathname === '/mcp') {
    if (admit(deps, req, res, pathname)) await handleMcp(deps, req, res);
  } else if (pathname === '/admin/refresh') {
    if (admit(deps, req, res, pathname)) {
      await handleAdmin(deps, req, res, new URL(req.url ?? '/', 'http://localhost'));
    }
  } else {
    sendJson(res, 404, { error: 'not found' });
  }
}

export function createHttpServer(deps: HttpDeps): Server {
  const server = createServer((req, res) => {
    const started = Date.now();
    res.on('finish', () => {
      deps.logger.info('http', {
        method: req.method,
        path: (req.url ?? '').split('?')[0],
        status: res.statusCode,
        ms: Date.now() - started,
      });
    });
    route(deps, req, res).catch((error: unknown) => {
      deps.logger.error('request failed', { error: errorMessage(error) });
      sendJson(res, 500, rpcError(-32603, 'Internal server error'));
    });
  });
  server.headersTimeout = 20_000;
  server.requestTimeout = 60_000;
  server.keepAliveTimeout = 5_000;
  return server;
}
