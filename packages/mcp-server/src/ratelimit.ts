/** In-memory token bucket per client key (docs/06-mcp-server.md "Security and limits"). */

export interface RateDecision {
  allowed: boolean;
  /** Seconds until one more request would be allowed; 0 when allowed. */
  retryAfterSec: number;
}

interface Bucket {
  tokens: number;
  updated: number;
}

const MAX_TRACKED_KEYS = 10_000;

export class RateLimiter {
  private readonly buckets = new Map<string, Bucket>();
  private readonly perSecond: number;
  private readonly burst: number;
  private readonly now: () => number;

  /** `perMin` of 0 disables limiting. */
  constructor(perMin: number, burst: number, now: () => number = Date.now) {
    this.perSecond = perMin / 60;
    this.burst = burst;
    this.now = now;
  }

  take(key: string): RateDecision {
    if (this.perSecond <= 0) return { allowed: true, retryAfterSec: 0 };
    const t = this.now();
    const bucket = this.buckets.get(key) ?? { tokens: this.burst, updated: t };
    bucket.tokens = Math.min(
      this.burst,
      bucket.tokens + ((t - bucket.updated) / 1000) * this.perSecond,
    );
    bucket.updated = t;
    this.buckets.delete(key);
    this.buckets.set(key, bucket);
    this.evict();

    if (bucket.tokens >= 1) {
      bucket.tokens -= 1;
      return { allowed: true, retryAfterSec: 0 };
    }
    return { allowed: false, retryAfterSec: Math.ceil((1 - bucket.tokens) / this.perSecond) };
  }

  /** Drops the least recently seen keys so a flood of addresses can't grow the map forever. */
  private evict(): void {
    while (this.buckets.size > MAX_TRACKED_KEYS) {
      const oldest = this.buckets.keys().next();
      if (oldest.done) return;
      this.buckets.delete(oldest.value);
    }
  }
}
