//! Per-client token-bucket rate limiting for the read routes and `POST /command`.
//!
//! This replaces `tower_governor`, whose `governor` dependency pulls in
//! `nonzero_ext`, which is Apache-2.0 only and banned by `deny.toml` (Phase 24e),
//! and every `governor` release still depends on it.
//!
//! Each client IP gets its own bucket. A bucket starts full (`burst` tokens),
//! refills at `per_second` tokens per second, and never holds more than `burst`.
//! A request takes one token; when none is left the response is `429 Too Many
//! Requests` with a `Retry-After` header.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Instant;

use http::{header, HeaderMap, HeaderValue, StatusCode};

use crate::RateLimitConfig;

/// Once this many clients are tracked, buckets that have refilled to full (and
/// so behave exactly like a missing bucket) are dropped. This bounds memory
/// under a flood of distinct source addresses.
const PRUNE_THRESHOLD: usize = 10_000;

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Keyed token-bucket limiter, shared by every request on one route.
pub(crate) struct KeyedRateLimiter {
    per_second: f64,
    burst: f64,
    trust_forwarded_for: bool,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}

impl KeyedRateLimiter {
    /// Builds a limiter from `config`. A `per_second` or `burst` of `0` is
    /// raised to `1`, since a zero quota would reject every request.
    pub(crate) fn new(config: RateLimitConfig, trust_forwarded_for: bool) -> Self {
        KeyedRateLimiter {
            per_second: config.per_second.max(1) as f64,
            burst: f64::from(config.burst.max(1)),
            trust_forwarded_for,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token from `key`'s bucket. On failure returns the number of
    /// whole seconds until the next token is available (at least 1).
    pub(crate) fn check(&self, key: IpAddr, now: Instant) -> Result<(), u64> {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() >= PRUNE_THRESHOLD && !buckets.contains_key(&key) {
            let (per_second, burst) = (self.per_second, self.burst);
            buckets.retain(|_, b| {
                let refilled =
                    b.tokens + now.saturating_duration_since(b.last).as_secs_f64() * per_second;
                refilled < burst
            });
        }
        let burst = self.burst;
        let bucket = buckets.entry(key).or_insert(Bucket {
            tokens: burst,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.last = now;
        bucket.tokens = (bucket.tokens + elapsed * self.per_second).min(self.burst);
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait = ((1.0 - bucket.tokens) / self.per_second).ceil().max(1.0);
            Err(wait as u64)
        }
    }

    /// Picks the client address to key on.
    ///
    /// With `trust_forwarded_for`, the first hop of `X-Forwarded-For`, then
    /// `X-Real-Ip`, is used, falling back to the peer address. Only enable that
    /// behind a proxy that sets those headers. Otherwise the peer socket address
    /// is used. Returns `None` if no address is available (the router was served
    /// without a peer address).
    pub(crate) fn client_key(&self, headers: &HeaderMap, peer: Option<IpAddr>) -> Option<IpAddr> {
        if self.trust_forwarded_for {
            if let Some(ip) = forwarded_ip(headers) {
                return Some(ip);
            }
        }
        peer
    }
}

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let first_hop = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim);
    first_hop.and_then(|v| v.parse().ok()).or_else(|| {
        headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok())
    })
}
impl KeyedRateLimiter {
    /// Checks one request against its client's bucket. `Err` carries the response
    /// to send: `429` with `Retry-After` when over the limit, or `500` when no client
    /// address can be determined (matching the previous behavior when the peer
    /// address is missing).
    pub(crate) fn check_request(
        &self,
        headers: &HeaderMap,
        peer: Option<IpAddr>,
    ) -> Result<(), Box<crate::app::Resp>> {
        let Some(key) = self.client_key(headers, peer) else {
            return Err(Box::new(crate::app::with_type(
                StatusCode::INTERNAL_SERVER_ERROR,
                "text/plain; charset=utf-8",
                "unable to determine client address for rate limiting",
            )));
        };
        match self.check(key, Instant::now()) {
            Ok(()) => Ok(()),
            Err(retry_after) => {
                let mut resp = Box::new(crate::app::with_type(
                    StatusCode::TOO_MANY_REQUESTS,
                    "text/plain; charset=utf-8",
                    "Too Many Requests",
                ));
                if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
                    resp.headers_mut().insert(header::RETRY_AFTER, value);
                }
                Err(resp)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::time::Duration;

    use super::*;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    fn limiter(per_second: u64, burst: u32) -> KeyedRateLimiter {
        KeyedRateLimiter::new(RateLimitConfig { per_second, burst }, false)
    }

    #[test]
    fn burst_then_limited_per_key() {
        let l = limiter(1, 2);
        let t0 = Instant::now();
        assert!(l.check(ip(1), t0).is_ok());
        assert!(l.check(ip(1), t0).is_ok());
        assert!(l.check(ip(1), t0).is_err());
        // Another client has its own full bucket.
        assert!(l.check(ip(2), t0).is_ok());
    }

    #[test]
    fn retry_after_is_at_least_one_second() {
        let l = limiter(10, 1);
        let t0 = Instant::now();
        assert!(l.check(ip(1), t0).is_ok());
        assert_eq!(l.check(ip(1), t0), Err(1));
    }

    #[test]
    fn tokens_refill_and_cap_at_burst() {
        let l = limiter(2, 2);
        let t0 = Instant::now();
        assert!(l.check(ip(1), t0).is_ok());
        assert!(l.check(ip(1), t0).is_ok());
        assert!(l.check(ip(1), t0).is_err());
        assert!(l.check(ip(1), t0 + Duration::from_millis(500)).is_ok());
        // A long idle period banks at most one burst.
        let later = t0 + Duration::from_secs(600);
        assert!(l.check(ip(1), later).is_ok());
        assert!(l.check(ip(1), later).is_ok());
        assert!(l.check(ip(1), later).is_err());
    }

    #[test]
    fn zero_config_is_treated_as_one() {
        let l = limiter(0, 0);
        let t0 = Instant::now();
        assert!(l.check(ip(1), t0).is_ok());
        assert!(l.check(ip(1), t0).is_err());
    }

    #[test]
    fn forwarded_for_is_ignored_unless_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        let untrusted = limiter(1, 1);
        assert_eq!(untrusted.client_key(&headers, Some(ip(7))), Some(ip(7)));
        let trusted = KeyedRateLimiter::new(
            RateLimitConfig {
                per_second: 1,
                burst: 1,
            },
            true,
        );
        assert_eq!(
            trusted.client_key(&headers, Some(ip(7))),
            Some("203.0.113.9".parse().unwrap())
        );
    }

    #[test]
    fn no_peer_and_no_header_yields_no_key() {
        let l = limiter(1, 1);
        assert_eq!(l.client_key(&HeaderMap::new(), None), None);
    }

    #[test]
    fn prunes_full_buckets_past_threshold() {
        let l = limiter(1000, 1);
        let t0 = Instant::now();
        for i in 0..PRUNE_THRESHOLD as u32 {
            let addr = IpAddr::V4(Ipv4Addr::from(i + 1));
            assert!(l.check(addr, t0).is_ok());
        }
        // Long after, every bucket has refilled: a new client triggers a prune.
        let later = t0 + Duration::from_secs(60);
        assert!(l
            .check(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), later)
            .is_ok());
        assert!(l.buckets.lock().unwrap().len() < PRUNE_THRESHOLD);
    }
}
