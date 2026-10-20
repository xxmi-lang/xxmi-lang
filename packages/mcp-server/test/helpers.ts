/** Test scaffolding: fixture repos served as tarballs through a fake GitHub client. */
import { mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { create } from 'tar';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { createApp, type App } from '../src/app.ts';
import { createMcpServer } from '../src/tools/index.ts';
import { loadConfig, type ServerConfig } from '../src/config.ts';
import { GitHubError, type GitHubClient, type RemoteRef } from '../src/github.ts';
import { silentLogger } from '../src/logger.ts';
import { TRACKED_REPOS, type TrackedRepo } from '../src/repos.ts';

export const FIXTURES = resolve(import.meta.dirname, 'fixtures');

/** The real tracked repos, served from fixtures by alias. */
export const TEST_REPOS: readonly TrackedRepo[] = TRACKED_REPOS;

export const SHA_A = 'a'.repeat(40);
export const SHA_B = 'b'.repeat(40);
export const TAG_SHA = 'c'.repeat(40);

export const tempDir = (): string => mkdtempSync(join(tmpdir(), 'xxmi-mcp-'));

export const FIXTURE_DIRS: Record<string, string> = {
  dll: join(FIXTURES, 'dll'),
  zzmi: join(FIXTURES, 'zzmi-package'),
  xxmitools: join(FIXTURES, 'xxmitools'),
};

export interface FakeRemote {
  head: RemoteRef;
  tags: RemoteRef[];
}

export interface FakeGitHub extends GitHubClient {
  remotes: Record<string, FakeRemote>;
  tarballRequests: string[];
  /** Makes the next tarball downloads of this repo fail. */
  failTarballs: Set<string>;
}

export function fakeGitHub(remotes: Record<string, FakeRemote>): FakeGitHub {
  const client: FakeGitHub = {
    remotes,
    tarballRequests: [],
    failTarballs: new Set(),
    head(repo) {
      const remote = client.remotes[repo.alias];
      return remote
        ? Promise.resolve(remote.head)
        : Promise.reject(new GitHubError(404, 'no repo'));
    },
    tags(repo, count) {
      return Promise.resolve(client.remotes[repo.alias]?.tags.slice(0, count) ?? []);
    },
    tarball(repo, sha) {
      client.tarballRequests.push(`${repo.alias}@${sha}`);
      if (client.failTarballs.has(repo.alias)) {
        return Promise.reject(new GitHubError(502, 'bad gateway'));
      }
      const dir = FIXTURE_DIRS[repo.alias];
      if (!dir) return Promise.reject(new GitHubError(404, 'no fixture'));
      // GitHub wraps the tree in a `<owner>-<repo>-<sha>/` folder.
      return Promise.resolve(
        Readable.from(
          create(
            { gzip: true, cwd: dir, prefix: `${repo.owner}-${repo.name}-${sha}` },
            readdirSync(dir),
          ),
        ),
      );
    },
  };
  return client;
}

export const defaultRemotes = (): Record<string, FakeRemote> => ({
  dll: { head: { name: 'master', sha: SHA_A }, tags: [] },
  zzmi: { head: { name: 'main', sha: SHA_A }, tags: [{ name: 'v1.0.0', sha: TAG_SHA }] },
  xxmitools: { head: { name: 'main', sha: SHA_A }, tags: [] },
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
  github: FakeGitHub;
}

/** An app over a freshly indexed fixture volume; the indexer ran once and does not poll. */
export async function indexedApp(overrides: Partial<ServerConfig> = {}): Promise<TestApp> {
  const github = fakeGitHub(defaultRemotes());
  const app = await createApp(testConfig(overrides), { github, logger: silentLogger });
  await app.indexer.runOnce();
  return { app, github };
}

/** An app that has not indexed anything: tools answer from the bundled snapshot. */
export function coldApp(overrides: Partial<ServerConfig> = {}): Promise<App> {
  return createApp(testConfig(overrides), {
    github: fakeGitHub(defaultRemotes()),
    logger: silentLogger,
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
