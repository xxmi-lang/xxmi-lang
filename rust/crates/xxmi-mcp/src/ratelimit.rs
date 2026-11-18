//! In-memory token bucket per client key (docs/06-mcp-server.md "Security and limits").

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

#[derive(Debug, PartialEq)]
pub struct RateDecision {
    pub allowed: bool,
    /// Seconds until one more request would be allowed; 0 when allowed.
    pub retry_after_sec: u64,
}

struct Bucket {
    tokens: f64,
    updated_ms: f64,
    /// Position in the recency order, so the least recently seen key is the first to go.
    seq: u64,
}

const MAX_TRACKED_KEYS: usize = 10_000;

pub struct RateLimiter {
    buckets: HashMap<String, Bucket>,
    recency: BTreeMap<u64, String>,
    next_seq: u64,
    per_second: f64,
    burst: f64,
    started: Instant,
}

impl RateLimiter {
    /// `per_min` of 0 disables limiting.
    pub fn new(per_min: u64, burst: u64) -> Self {
        Self {
            buckets: HashMap::new(),
            recency: BTreeMap::new(),
            next_seq: 0,
            per_second: per_min as f64 / 60.0,
            burst: burst as f64,
            started: Instant::now(),
        }
    }

    pub fn take(&mut self, key: &str) -> RateDecision {
        let now_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.take_at(key, now_ms)
    }

    pub fn take_at(&mut self, key: &str, now_ms: f64) -> RateDecision {
        if self.per_second <= 0.0 {
            return RateDecision {
                allowed: true,
                retry_after_sec: 0,
            };
        }

        let mut bucket = match self.buckets.remove(key) {
            Some(bucket) => {
                self.recency.remove(&bucket.seq);
                bucket
            }
            None => Bucket {
                tokens: self.burst,
                updated_ms: now_ms,
                seq: 0,
            },
        };
        bucket.tokens = self
            .burst
            .min(bucket.tokens + (now_ms - bucket.updated_ms) / 1000.0 * self.per_second);
        bucket.updated_ms = now_ms;

        let decision = if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            RateDecision {
                allowed: true,
                retry_after_sec: 0,
            }
        } else {
            let wait = ((1.0 - bucket.tokens) / self.per_second).ceil();
            RateDecision {
                allowed: false,
                retry_after_sec: wait as u64,
            }
        };

        bucket.seq = self.next_seq;
        self.next_seq += 1;
        self.recency.insert(bucket.seq, key.to_string());
        self.buckets.insert(key.to_string(), bucket);
        self.evict();
        decision
    }

    /// Drops the least recently seen keys so a flood of addresses can't grow the map forever.
    fn evict(&mut self) {
        while self.buckets.len() > MAX_TRACKED_KEYS {
            let Some((_, oldest)) = self.recency.pop_first() else {
                return;
            };
            self.buckets.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spends_the_burst_then_refills_over_time() {
        let mut limiter = RateLimiter::new(60, 2);
        assert!(limiter.take_at("a", 0.0).allowed);
        assert!(limiter.take_at("a", 0.0).allowed);

        let denied = limiter.take_at("a", 0.0);
        assert_eq!(
            denied,
            RateDecision {
                allowed: false,
                retry_after_sec: 1
            }
        );
        assert!(limiter.take_at("a", 1000.0).allowed);
    }

    #[test]
    fn keys_do_not_share_a_budget() {
        let mut limiter = RateLimiter::new(60, 1);
        assert!(limiter.take_at("a", 0.0).allowed);
        assert!(limiter.take_at("b", 0.0).allowed);
        assert!(!limiter.take_at("a", 0.0).allowed);
    }

    #[test]
    fn zero_per_minute_disables_the_limit() {
        let mut limiter = RateLimiter::new(0, 1);
        assert!((0..100).all(|_| limiter.take_at("a", 0.0).allowed));
    }

    #[test]
    fn tracked_keys_are_bounded() {
        let mut limiter = RateLimiter::new(60, 5);
        for n in 0..MAX_TRACKED_KEYS + 50 {
            limiter.take_at(&n.to_string(), 0.0);
        }
        assert_eq!(limiter.buckets.len(), MAX_TRACKED_KEYS);
        assert!(!limiter.buckets.contains_key("0"));
    }
}
