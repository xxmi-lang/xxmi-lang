import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import { afterEach, describe, expect, it } from 'vitest';
import type { App } from '../src/app.ts';
import { indexedApp } from './helpers.ts';

const apps: App[] = [];
afterEach(async () => {
  await Promise.all(apps.splice(0).map((a) => a.close()));
});

async function start(overrides = {}): Promise<{ app: App; base: string }> {
  const { app } = await indexedApp(overrides);
  apps.push(app);
  const port = await app.listen();
  return { app, base: `http://127.0.0.1:${port}` };
}

const rpc = (method: string, params: object = {}, id = 1): string =>
  JSON.stringify({ jsonrpc: '2.0', id, method, params });

const MCP_HEADERS = {
  'content-type': 'application/json',
  accept: 'application/json, text/event-stream',
};

describe('HTTP server', () => {
  it('answers /healthz with the ready flag', async () => {
    const { base } = await start();

    const res = await fetch(`${base}/healthz`);

    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ status: 'ok', ready: true });
  });

  it('speaks Streamable HTTP to the official client', async () => {
    const { base } = await start();
    const client = new Client({ name: 'test', version: '1' });
    // The SDK's client transport doesn't type-check under exactOptionalPropertyTypes.
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`${base}/mcp`)) as Parameters<Client['connect']>[0],
    );

    const tools = await client.listTools();
    const result = await client.callTool({
      name: 'spec_lookup',
      arguments: { name: 'store' },
    });

    expect(tools.tools.map((t) => t.name)).toContain('lint_ini');
    expect(JSON.stringify(result)).toContain('XXMI-only');
    await client.close();
  });

  it('is stateless: GET and DELETE on /mcp are 405, unknown paths 404', async () => {
    const { base } = await start();

    expect((await fetch(`${base}/mcp`)).status).toBe(405);
    expect((await fetch(`${base}/mcp`, { method: 'DELETE' })).status).toBe(405);
    expect((await fetch(`${base}/nope`)).status).toBe(404);
  });

  it('rejects invalid JSON with 400', async () => {
    const { base } = await start();

    const res = await fetch(`${base}/mcp`, { method: 'POST', headers: MCP_HEADERS, body: '{nope' });

    expect(res.status).toBe(400);
  });
});

describe('rate limiting', () => {
  it('answers 429 with Retry-After once the burst is used, per client', async () => {
    const { base } = await start({ rateLimitPerMin: 60, rateLimitBurst: 3 });
    const post = (headers = {}): Promise<Response> =>
      fetch(`${base}/mcp`, {
        method: 'POST',
        headers: { ...MCP_HEADERS, ...headers },
        body: rpc('tools/list'),
      });

    const statuses: number[] = [];
    for (let i = 0; i < 5; i++) statuses.push((await post()).status);
    const refused = await post();

    expect(statuses).toEqual([200, 200, 200, 429, 429]);
    expect(Number(refused.headers.get('retry-after'))).toBeGreaterThanOrEqual(1);
    expect((await fetch(`${base}/healthz`)).status).toBe(200);
  });

  it('keys on X-Forwarded-For behind a trusted proxy, ignoring spoofed leading entries', async () => {
    const { base } = await start({ rateLimitPerMin: 60, rateLimitBurst: 1, trustedProxyHops: 1 });
    const post = (forwarded: string): Promise<Response> =>
      fetch(`${base}/mcp`, {
        method: 'POST',
        headers: { ...MCP_HEADERS, 'x-forwarded-for': forwarded },
        body: rpc('tools/list'),
      });

    const first = await post('1.1.1.1, 9.9.9.9');
    const spoofed = await post('2.2.2.2, 9.9.9.9');
    const other = await post('1.1.1.1, 8.8.8.8');

    expect([first.status, spoofed.status, other.status]).toEqual([200, 429, 200]);
  });
});

describe('input caps', () => {
  it('refuses request bodies over MAX_BODY_BYTES with 413', async () => {
    const { base } = await start({ maxBodyBytes: 2048 });

    const res = await fetch(`${base}/mcp`, {
      method: 'POST',
      headers: MCP_HEADERS,
      body: rpc('tools/call', {
        name: 'lint_ini',
        arguments: { files: { 'a.ini': 'x'.repeat(5000) } },
      }),
    });

    expect(res.status).toBe(413);
  });

  it('refuses lint input over MAX_LINT_BYTES as a tool error', async () => {
    const { base } = await start({ maxLintBytes: 1000 });

    const res = await fetch(`${base}/mcp`, {
      method: 'POST',
      headers: MCP_HEADERS,
      body: rpc('tools/call', {
        name: 'lint_ini',
        arguments: { files: { 'a.ini': '; '.repeat(600) } },
      }),
    });
    const body = (await res.json()) as {
      result: { isError: boolean; content: { text: string }[] };
    };

    expect(body.result.isError).toBe(true);
    expect(body.result.content[0]?.text).toContain('input too large');
  });
});

describe('/admin/refresh', () => {
  it('does not exist without ADMIN_TOKEN', async () => {
    const { base } = await start();

    expect((await fetch(`${base}/admin/refresh`, { method: 'POST' })).status).toBe(404);
  });

  it('needs the bearer token and reports the pass with ?wait=1', async () => {
    const { base } = await start({ adminToken: 's3cret' });
    const refresh = (token?: string): Promise<Response> =>
      fetch(`${base}/admin/refresh?wait=1`, {
        method: 'POST',
        headers: token ? { authorization: `Bearer ${token}` } : {},
      });

    expect((await refresh()).status).toBe(401);
    expect((await refresh('wrong')).status).toBe(401);
    const ok = await refresh('s3cret');
    expect(ok.status).toBe(200);
    expect(await ok.json()).toMatchObject({ errors: [] });
  });
});
