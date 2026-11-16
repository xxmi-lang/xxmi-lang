import { describe, expect, it } from 'vitest';
import { ExecError, MAX_OUTPUT_BYTES, run, stream } from '../src/exec.ts';

const node = process.execPath;
const options = (timeoutMs: number) => ({ timeoutMs, label: 'probe' });

async function failureOf(promise: Promise<unknown>): Promise<ExecError> {
  const error: unknown = await promise.then(
    () => undefined,
    (e: unknown) => e,
  );
  if (!(error instanceof ExecError)) throw new Error('expected an ExecError');
  return error;
}

describe('run', () => {
  it('reports a timeout as one', async () => {
    const error = await failureOf(run(node, ['-e', 'setTimeout(() => {}, 10000)'], options(200)));

    expect(error.message).toMatch(/timed out after 200 ms/);
    expect(error.timedOut).toBe(true);
  });

  it('reports exceeding the output cap as that, not as a timeout', async () => {
    const script = `process.stdout.write('x'.repeat(${MAX_OUTPUT_BYTES + 1000}))`;

    const error = await failureOf(run(node, ['-e', script], options(30_000)));

    expect(error.message).toMatch(/output exceeded/);
    expect(error.message).not.toMatch(/timed out/);
    expect(error.timedOut).toBe(false);
  });

  it("carries the child's stderr", async () => {
    const error = await failureOf(
      run(node, ['-e', "console.error('boom'); process.exit(3)"], options(30_000)),
    );

    expect(error.message).toMatch(/boom/);
  });
});

describe('stream', () => {
  it('reports a timeout as one', async () => {
    const child = stream(node, ['-e', 'setTimeout(() => {}, 10000)'], options(200));
    child.stdout.resume();

    const error = await failureOf(child.exit);

    expect(error.timedOut).toBe(true);
  });
});
