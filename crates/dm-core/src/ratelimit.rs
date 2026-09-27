//! Bandwidth limiting shared by every connection that draws from it.
//!
//! One limiter caps the whole application; another can cap a single task
//! (from a matching rule). Each connection asks for the bytes it has just
//! written; a connection that runs ahead of the budget sleeps until the
//! budget has caught up. The budget is a token bucket that may go into debt,
//! so many connections drawing at once still add up to the configured rate
//! instead of each getting the full rate.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// A short burst allowance keeps small files and the start of a transfer
/// from stalling, without letting the average exceed the limit.
const BURST: Duration = Duration::from_millis(250);
const MIN_BURST_BYTES: f64 = 16.0 * 1024.0;

#[derive(Debug)]
pub struct RateLimiter {
    /// Bytes per second. Zero means unlimited.
    limit: AtomicU64,
    bucket: Mutex<Bucket>,
}

#[derive(Debug)]
struct Bucket {
    available: f64,
    refreshed_at: Instant,
}

impl RateLimiter {
    pub fn new(limit: Option<u64>) -> Self {
        let limiter = Self {
            limit: AtomicU64::new(0),
            bucket: Mutex::new(Bucket {
                available: 0.0,
                refreshed_at: Instant::now(),
            }),
        };
        limiter.set_limit(limit);
        limiter
    }

    pub fn unlimited() -> Self {
        Self::new(None)
    }

    /// Changes the limit for every connection at once, including transfers
    /// already running. `None` or zero removes the limit.
    pub fn set_limit(&self, limit: Option<u64>) {
        let limit = limit.unwrap_or(0);
        self.limit.store(limit, Ordering::Release);

        if let Ok(mut bucket) = self.bucket.lock() {
            bucket.available = burst_bytes(limit);
            bucket.refreshed_at = Instant::now();
        }
    }

    pub fn limit(&self) -> Option<u64> {
        match self.limit.load(Ordering::Acquire) {
            0 => None,
            limit => Some(limit),
        }
    }

    /// Accounts for `bytes` just transferred and waits until they fit the
    /// limit. Returns at once when unlimited.
    pub async fn acquire(&self, bytes: usize) {
        let delay = self.reserve(bytes, Instant::now());
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    /// Counts `bytes` another program already transferred (yt-dlp, which
    /// limits itself), without waiting. Connections drawing from this
    /// limiter then slow down to leave room for them.
    pub fn record(&self, bytes: usize) {
        let _ = self.reserve(bytes, Instant::now());
    }

    /// Takes `bytes` from the bucket and returns how long the caller must
    /// wait before the budget is back out of debt.
    fn reserve(&self, bytes: usize, now: Instant) -> Duration {
        let limit = self.limit.load(Ordering::Acquire);
        if limit == 0 {
            return Duration::ZERO;
        }

        let Ok(mut bucket) = self.bucket.lock() else {
            return Duration::ZERO;
        };

        let rate = limit as f64;
        let elapsed = now.saturating_duration_since(bucket.refreshed_at);
        bucket.available =
            (bucket.available + elapsed.as_secs_f64() * rate).min(burst_bytes(limit));
        bucket.refreshed_at = now;
        bucket.available -= bytes as f64;

        if bucket.available >= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(-bucket.available / rate)
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::unlimited()
    }
}

fn burst_bytes(limit: u64) -> f64 {
    (limit as f64 * BURST.as_secs_f64()).max(MIN_BURST_BYTES)
}

#[cfg(test)]
mod tests {
    use super::RateLimiter;
    use std::time::{Duration, Instant};

    #[test]
    fn unlimited_never_waits() {
        let limiter = RateLimiter::unlimited();
        assert_eq!(limiter.reserve(10_000_000, Instant::now()), Duration::ZERO);
    }

    #[test]
    fn debt_is_repaid_at_the_configured_rate() {
        let limiter = RateLimiter::new(Some(100_000));
        let now = Instant::now();

        // The burst allowance (25 000 bytes at this rate) is free.
        assert_eq!(limiter.reserve(25_000, now), Duration::ZERO);
        // The next 100 000 bytes must take one second.
        let wait = limiter.reserve(100_000, now);
        assert!((wait.as_secs_f64() - 1.0).abs() < 0.01, "{wait:?}");
    }

    #[test]
    fn concurrent_callers_share_one_budget() {
        let limiter = RateLimiter::new(Some(100_000));
        let now = Instant::now();
        limiter.reserve(25_000, now);

        // Two connections taking 50 000 bytes each: the second waits for
        // both, so together they still move 100 000 bytes per second.
        let first = limiter.reserve(50_000, now);
        let second = limiter.reserve(50_000, now);
        assert!((first.as_secs_f64() - 0.5).abs() < 0.01);
        assert!((second.as_secs_f64() - 1.0).abs() < 0.01);
    }

    #[test]
    fn raising_or_removing_the_limit_takes_effect_at_once() {
        let limiter = RateLimiter::new(Some(1_000));
        let now = Instant::now();
        assert!(limiter.reserve(100_000, now) > Duration::from_secs(10));

        limiter.set_limit(None);
        assert_eq!(limiter.limit(), None);
        assert_eq!(limiter.reserve(100_000, now), Duration::ZERO);
    }

    #[tokio::test]
    async fn limited_transfer_takes_the_expected_time() {
        let limiter = RateLimiter::new(Some(200_000));
        let started = Instant::now();
        // Burst 50 000 + 100 000 more at 200 000 B/s = 0.5 s.
        for _ in 0..15 {
            limiter.acquire(10_000).await;
        }
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(450), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(900), "{elapsed:?}");
    }
}
