/** Wires config, volume, catalog, indexer and HTTP server together. */
import type { Server } from 'node:http';
import { bundledSnapshots, defaultSpec } from '@xxmi-lang/core';
import { Catalog } from './catalog.ts';
import type { ServerConfig } from './config.ts';
import { createHttpServer } from './http.ts';
import { Indexer } from './indexer.ts';
import { createLogger, type Logger } from './logger.ts';
import { PackageCache } from './packages.ts';
import { RateLimiter } from './ratelimit.ts';
import { TRACKED_REPOS, type TrackedRepo } from './repos.ts';
import { builderId } from './specs/build.ts';
import type { SpecBuilder } from './specs/ensure.ts';
import { SpecRegistry } from './specs/registry.ts';
import { SpecStore } from './specs/store.ts';
import { Store } from './store.ts';
import type { ToolContext } from './tools/shared.ts';
import { createSourceClient, type SourceClient } from './upstream.ts';

export interface AppOptions {
  sources?: SourceClient;
  logger?: Logger;
  repos?: readonly TrackedRepo[];
  now?: () => number;
  /** Replaces the worker-thread spec build (tests). */
  buildSpec?: SpecBuilder;
  /** Replaces `builderId()` (tests of the rebuild rules). */
  builder?: string;
}

export interface App {
  server: Server;
  indexer: Indexer;
  catalog: Catalog;
  tools: ToolContext;
  /** Starts listening (port 0 picks a free one) and the polling indexer; returns the port. */
  listen(): Promise<number>;
  close(): Promise<void>;
}

export async function createApp(config: ServerConfig, options: AppOptions = {}): Promise<App> {
  const logger = options.logger ?? createLogger();
  const repos = options.repos ?? TRACKED_REPOS;
  const store = new Store(config.dataDir);
  await store.init();
  const catalog = new Catalog(store, repos);
  await catalog.load();

  const specs = new SpecRegistry({
    store: new SpecStore(store),
    catalog,
    bundled: defaultSpec(),
    builder: options.builder ?? builderId(),
    repos,
  });
  const tools: ToolContext = {
    config,
    catalog,
    packages: new PackageCache(store, specs.bundled.lookup),
    logger,
    specs,
    bundled: (game) => bundledSnapshots().find((s) => s.name === game),
  };
  const indexer = new Indexer({
    store,
    catalog,
    sources:
      options.sources ??
      createSourceClient({ dataDir: config.dataDir, token: config.githubToken, logger }),
    logger,
    repos,
    lookup: specs.bundled.lookup,
    specs,
    ...(options.buildSpec ? { buildSpec: options.buildSpec } : {}),
    limits: { releases: config.indexedReleases, prereleases: config.indexedPrereleases },
    gitCacheMaxMb: config.gitCacheMaxMb,
    pollIntervalMin: config.pollIntervalMin,
  });
  const limiter = new RateLimiter(config.rateLimitPerMin, config.rateLimitBurst, options.now);
  const server = createHttpServer({ config, catalog, indexer, limiter, logger, tools });

  return {
    server,
    indexer,
    catalog,
    tools,
    listen() {
      return new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(config.port, config.host, () => {
          const address = server.address();
          const port = typeof address === 'object' && address ? address.port : config.port;
          logger.info('listening', { port, ready: catalog.ready, indexer: config.indexerEnabled });
          if (config.indexerEnabled) indexer.start();
          resolve(port);
        });
      });
    },
    close() {
      indexer.stop();
      return new Promise((resolve) => {
        server.close(() => {
          resolve();
        });
        server.closeAllConnections();
      });
    },
  };
}
