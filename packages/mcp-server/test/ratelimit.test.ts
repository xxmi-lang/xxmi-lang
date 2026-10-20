import { describe, expect, it } from 'vitest';
import { RateLimiter } from '../src/ratelimit.ts';

describe('RateLimiter', () => {
  it('allows a burst, then refuses with a retry hint', () => {
    let now = 0;
    const limiter = new RateLimiter(60, 3, () => now);

    expect([1, 2, 3].map(() => limiter.take('a').allowed)).toEqual([true, true, true]);
    const refused = limiter.take('a');

    expect(refused.allowed).toBe(false);
    expect(refused.retryAfterSec).toBe(1);

    now += 1000;
    expect(limiter.take('a').allowed).toBe(true);
  });

  it('limits each client separately', () => {
    const limiter = new RateLimiter(60, 1, () => 0);

    expect(limiter.take('a').allowed).toBe(true);
    expect(limiter.take('b').allowed).toBe(true);
    expect(limiter.take('a').allowed).toBe(false);
  });

  it('never exceeds the burst after a long idle time', () => {
    let now = 0;
    const limiter = new RateLimiter(60, 2, () => now);
    limiter.take('a');
    now += 3_600_000;

    expect([1, 2, 3].map(() => limiter.take('a').allowed)).toEqual([true, true, false]);
  });

  it('is off when perMin is 0', () => {
    const limiter = new RateLimiter(0, 1, () => 0);

    expect(Array.from({ length: 100 }, () => limiter.take('a').allowed).every(Boolean)).toBe(true);
  });
});
