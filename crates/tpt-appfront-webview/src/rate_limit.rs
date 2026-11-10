//! A single-bucket token-bucket rate limiter for the IPC bridge.
//!
//! This replaces `governor`, whose dependency `nonzero_ext` is Apache-2.0 only
//! and banned by `deny.toml` (Phase 24e). The IPC bridge has one hosted page in
//! one local process, so one bucket is all it needs.

use std::sync::Mutex;
use std::time::Instant;

/// Allows `per_second` events per second in steady state, with a burst of up to
/// `per_second` events.
pub(crate) struct TokenBucket {
    per_second: f64,
    burst: f64,
    state: Mutex<BucketState>,
}

struct BucketState {
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    /// Creates a bucket that starts full. A rate of `0` is treated as `1`, since
    /// a zero quota would reject every event.
    pub(crate) fn per_second(per_second: u32) -> Self {
        let n = f64::from(per_second.max(1));
        TokenBucket {
            per_second: n,
            burst: n,
            state: Mutex::new(BucketState {
                tokens: n,
                last: Instant::now(),
            }),
        }
    }

    /// Takes one token if one is available. Returns `Err(())` when the caller
    /// is over the limit.
    pub(crate) fn check(&self) -> Result<(), ()> {
        self.check_at(Instant::now())
    }

    fn check_at(&self, now: Instant) -> Result<(), ()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = now.saturating_duration_since(state.last).as_secs_f64();
        state.last = now;
        state.tokens = (state.tokens + elapsed * self.per_second).min(self.burst);
        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            Ok(())
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn burst_is_allowed_then_limited() {
        let bucket = TokenBucket::per_second(3);
        let t0 = Instant::now();
        assert!(bucket.check_at(t0).is_ok());
        assert!(bucket.check_at(t0).is_ok());
        assert!(bucket.check_at(t0).is_ok());
        assert!(bucket.check_at(t0).is_err());
    }

    #[test]
    fn tokens_refill_over_time() {
        let bucket = TokenBucket::per_second(2);
        let t0 = Instant::now();
        assert!(bucket.check_at(t0).is_ok());
        assert!(bucket.check_at(t0).is_ok());
        assert!(bucket.check_at(t0).is_err());
        // Half a second at 2/s replenishes one token.
        assert!(bucket.check_at(t0 + Duration::from_millis(500)).is_ok());
        assert!(bucket.check_at(t0 + Duration::from_millis(500)).is_err());
    }

    #[test]
    fn refill_never_exceeds_burst() {
        let bucket = TokenBucket::per_second(2);
        let t0 = Instant::now();
        // A long idle period must not bank more than one burst.
        let later = t0 + Duration::from_secs(60);
        assert!(bucket.check_at(later).is_ok());
        assert!(bucket.check_at(later).is_ok());
        assert!(bucket.check_at(later).is_err());
    }

    #[test]
    fn zero_rate_is_treated_as_one() {
        let bucket = TokenBucket::per_second(0);
        let t0 = Instant::now();
        assert!(bucket.check_at(t0).is_ok());
        assert!(bucket.check_at(t0).is_err());
    }
}
