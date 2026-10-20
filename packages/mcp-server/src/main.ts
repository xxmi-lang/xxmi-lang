#!/usr/bin/env node
/** `xxmi-mcp`: the hosted MCP server (docs/06-mcp-server.md). Configuration is environment-only. */
import { relaunchWithWasmFlags } from '@xxmi-lang/core';
import { createApp } from './app.ts';
import { loadConfig } from './config.ts';
import { createLogger, errorMessage } from './logger.ts';

async function main(): Promise<void> {
  const logger = createLogger();
  const config = loadConfig();
  const app = await createApp(config, { logger });
  await app.listen();

  const shutdown = (signal: string): void => {
    logger.info('shutting down', { signal });
    void app.close().then(() => process.exit(0));
    setTimeout(() => process.exit(0), 10_000).unref();
  };
  process.on('SIGTERM', () => {
    shutdown('SIGTERM');
  });
  process.on('SIGINT', () => {
    shutdown('SIGINT');
  });
}

// Under Node, web-tree-sitter leaks without --liftoff-only (packages/core/src/runtime.ts).
if (import.meta.main && !(await relaunchWithWasmFlags())) {
  main().catch((error: unknown) => {
    console.error(
      JSON.stringify({ level: 'error', msg: 'startup failed', error: errorMessage(error) }),
    );
    process.exitCode = 1;
  });
}
