/** Runs one spec build in a worker thread that is terminated when it takes too long. */
import { extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Worker } from 'node:worker_threads';
import type { BuildInput, BuiltSpec } from './build.ts';

export type WorkerReply = { ok: true; built: BuiltSpec } | { ok: false; error: string };

export const BUILD_TIMEOUT_MS = 120_000;
const MAX_HEAP_MB = 512;

/** `.ts` when running from source (`--conditions=source`), `.js` from the build output. */
const EXTENSION = extname(fileURLToPath(import.meta.url));
const WORKER_URL = new URL(`./worker${EXTENSION}`, import.meta.url);

export interface RunnerOptions {
  timeoutMs?: number;
  /** Replaces the build thread; tests point it at a script that never answers. */
  workerUrl?: URL;
}

export function buildInWorker(input: BuildInput, options: RunnerOptions = {}): Promise<BuiltSpec> {
  const timeoutMs = options.timeoutMs ?? BUILD_TIMEOUT_MS;
  return new Promise((resolve, reject) => {
    const worker = new Worker(options.workerUrl ?? WORKER_URL, {
      workerData: input,
      resourceLimits: { maxOldGenerationSizeMb: MAX_HEAP_MB },
      // The workspace packages resolve to their sources in dev and tests, like the main thread.
      ...(EXTENSION === '.ts' ? { execArgv: ['--conditions=source'] } : {}),
    });
    // Whichever event comes first settles the promise; the thread is stopped in every case.
    const finish = (settle: () => void): void => {
      clearTimeout(timer);
      void worker.terminate();
      settle();
    };
    const timer = setTimeout(() => {
      finish(() => {
        reject(new Error(`spec build timed out after ${timeoutMs} ms`));
      });
    }, timeoutMs);

    worker.once('message', (reply: WorkerReply) => {
      finish(() => {
        if (reply.ok) resolve(reply.built);
        else reject(new Error(reply.error));
      });
    });
    worker.once('error', (error) => {
      finish(() => {
        reject(error);
      });
    });
    worker.once('exit', (code) => {
      finish(() => {
        reject(new Error(`spec build worker exited with code ${code}`));
      });
    });
  });
}
