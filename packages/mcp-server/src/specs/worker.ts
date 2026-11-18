/**
 * Thread entry of a spec build: extraction is synchronous and blocks for a few hundred
 * milliseconds, which would stall every HTTP request if it ran on the server's own thread.
 */
import { parentPort, workerData } from 'node:worker_threads';
import { errorMessage } from '../logger.ts';
import { buildVersionSpec, type BuildInput } from './build.ts';
import type { WorkerReply } from './runner.ts';

function reply(message: WorkerReply): void {
  parentPort?.postMessage(message);
}

try {
  reply({ ok: true, built: buildVersionSpec(workerData as BuildInput) });
} catch (error) {
  reply({ ok: false, error: errorMessage(error) });
}
