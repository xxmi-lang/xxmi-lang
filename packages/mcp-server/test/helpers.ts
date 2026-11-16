/** Test scaffolding: fixture repos served as extracted trees through a fake source client. */
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { create } from 'tar';
import { afterAll } from 'vitest';
import { defaultSpec, SpecLookup } from '@xxmi-lang/core';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { createApp, type App, type AppOptions } from '../src/app.ts';
import { createMcpServer } from '../src/tools/index.ts';
import { loadConfig, type ServerConfig } from '../src/config.ts';
import { extractTar } from '../src/extract.ts';
import { buildVersionSpec } from '../src/specs/build.ts';
import type { SpecBuilder } from '../src/specs/ensure.ts';
import { silentLogger } from '../src/logger.ts';
import type { ReleaseInfo } from '../src/releases.ts';
import { TRACKED_REPOS, type TrackedRepo } from '../src/repos.ts';
import type { SourceClient } from '../src/upstream.ts';

export const FIXTURES = resolve(import.meta.dirname, 'fixtures');

/** The real tracked repos, served from fixtures by alias. */
export const TEST_REPOS: readonly TrackedRepo[] = TRACKED_REPOS;

export const SHA_A = 'a'.repeat(40);
export const SHA_B = 'b'.repeat(40);
export const TAG_SHA = 'c'.repeat(40);

export const TEMP_PREFIX = 'xxmi-mcp-';

const tempDirs: string[] = [];

afterAll(() => {
  for (const dir of tempDirs.splice(0))
    rmSync(dir, { recursive: true, force: true, maxRetries: 3 });
});

/** Polls until `condition` holds, so a slow runner delays a test instead of failing it. */
export async function waitFor(condition: () => boolean, timeoutMs = 30_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(`condition not met within ${timeoutMs} ms`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

/** A fresh directory that is removed when the test file finishes. */
export function tempDir(): string {
  const dir = mkdtempSync(join(tmpdir(), TEMP_PREFIX));
  tempDirs.push(dir);
  return dir;
}

export const FIXTURE_DIRS: Record<string, string> = {
  dll: join(FIXTURES, 'dll'),
  zzmi: join(FIXTURES, 'zzmi-package'),
  xxmitools: join(FIXTURES, 'xxmitools'),
  migoto: join(FIXTURES, 'dll'),
};

export interface FakeRef {
  name: string;
  sha: string;
}

export interface FakeRemote {
  head: FakeRef;
  tags: FakeRef[];
  /** What the releases lookup answers; none by default. */
  releases?: ReleaseInfo[];
}

export interface FakeSources extends SourceClient {
  remotes: Record<string, FakeRemote>;
  /** `alias@sha` of every materialized commit. */
  materialized: string[];
  /** `alias` of every releases lookup. */
  releaseCalls: string[];
  /** Makes the next materializations of this repo fail. */
  failMaterialize: Set<string>;
}

/** The lookup package checkouts are parsed with in tests (the bundled spec). */
export const bundledLookup = (): SpecLookup => new SpecLookup(defaultSpec());

/** Builds in the test's own thread: a worker per indexed app would only slow the suite down. */
export const inlineBuild: SpecBuilder = (input) => Promise.resolve(buildVersionSpec(input));

/**
 * Checkout directories by `alias`, or by `alias@sha` to give one commit other sources than
 * the rest of its repo.
 */
export type FixtureDirs = Record<string, string>;

export function fakeSources(
  remotes: Record<string, FakeRemote>,
  dirs: FixtureDirs = FIXTURE_DIRS,
): FakeSources {
  const client: FakeSources = {
    remotes,
    materialized: [],
    releaseCalls: [],
    failMaterialize: new Set(),
    listRefs(repo) {
      const remote = client.remotes[repo.alias];
      if (!remote) return Promise.reject(new Error('no repo'));
      return Promise.resolve({
        defaultBranch: remote.head.name,
        head: remote.head.sha,
        tags: remote.tags,
      });
    },
    commitInfo(_repo, sha) {
      return Promise.resolve({
        date: '2026-01-01T00:00:00Z',
        subject: `commit ${sha.slice(0, 7)}`,
      });
    },
    async materialize(repo, sha, destDir) {
      client.materialized.push(`${repo.alias}@${sha}`);
      if (client.failMaterialize.has(repo.alias)) throw new Error('bad gateway');
      const dir = dirs[`${repo.alias}@${sha}`] ?? dirs[repo.alias];
      if (!dir) throw new Error('no fixture');
      return extractTar(Readable.from(create({ cwd: dir }, readdirSync(dir))), destDir, repo.role);
    },
    releases(repo) {
      client.releaseCalls.push(repo.alias);
      return Promise.resolve(client.remotes[repo.alias]?.releases ?? []);
    },
    trimCache: () => Promise.resolve(false),
  };
  return client;
}

export const defaultRemotes = (): Record<string, FakeRemote> => ({
  dll: { head: { name: 'master', sha: SHA_A }, tags: [] },
  zzmi: { head: { name: 'main', sha: SHA_A }, tags: [{ name: 'v1.0.0', sha: TAG_SHA }] },
  xxmitools: { head: { name: 'main', sha: SHA_A }, tags: [] },
  migoto: { head: { name: 'master', sha: SHA_A }, tags: [] },
});

export function testConfig(overrides: Partial<ServerConfig> = {}): ServerConfig {
  return {
    ...loadConfig({ DATA_DIR: tempDir(), HOST: '127.0.0.1', PORT: '1' }),
    port: 0,
    pollIntervalMin: 0,
    indexerEnabled: false,
    rateLimitPerMin: 0,
    ...overrides,
  };
}

export { silentLogger };

export interface TestApp {
  app: App;
  sources: FakeSources;
}

/** An app over a freshly indexed fixture volume; the indexer ran once and does not poll. */
export async function indexedApp(
  overrides: Partial<ServerConfig> = {},
  remotes: Record<string, FakeRemote> = defaultRemotes(),
  options: { dirs?: FixtureDirs; app?: AppOptions } = {},
): Promise<TestApp> {
  const sources = fakeSources(remotes, options.dirs);
  const app = await createApp(testConfig(overrides), {
    sources,
    logger: silentLogger,
    buildSpec: inlineBuild,
    ...options.app,
  });
  await app.indexer.runOnce();
  return { app, sources };
}

/** An app that has not indexed anything: tools answer from the bundled snapshot. */
export function coldApp(overrides: Partial<ServerConfig> = {}): Promise<App> {
  return createApp(testConfig(overrides), {
    sources: fakeSources(defaultRemotes()),
    logger: silentLogger,
    buildSpec: inlineBuild,
  });
}

/** In-process MCP client wired to a fresh server for `app` (no HTTP). */
export async function connectClient(app: App): Promise<Client> {
  const server = createMcpServer(app.tools);
  const [clientSide, serverSide] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'test', version: '1' });
  await Promise.all([server.connect(serverSide), client.connect(clientSide)]);
  return client;
}

/** Text of a tool call, plus whether it was an error. */
export async function callTool(
  client: Client,
  name: string,
  args: Record<string, unknown>,
): Promise<{ text: string; isError: boolean }> {
  const result = await client.callTool({ name, arguments: args });
  const content = result['content'] as { type: string; text: string }[];
  return { text: content.map((c) => c.text).join('\n'), isError: result['isError'] === true };
}
