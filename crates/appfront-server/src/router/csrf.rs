//! Double-submit-cookie CSRF protection for `POST /command`.
//!
//! The double-submit pattern doesn't need the token itself to be
//! cryptographically secret: it relies on the browser's same-origin policy
//! (a cross-site attacker page can neither read this cookie nor attach a
//! matching custom header to its forged request), not on the token being
//! unguessable. Enforcement only kicks in when the request already carries
//! the cookie — a direct API/agent caller that never loaded a page from this
//! server (and so never received the cookie) is unaffected.

use axum::http::{HeaderMap, HeaderValue};

pub(crate) const COOKIE_NAME: &str = "appfront_csrf";
pub(crate) const HEADER_NAME: &str = "x-csrf-token";

/// Generates a fresh token for the `Set-Cookie` header on a document
/// response.
pub(crate) fn generate_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:016x}{c:016x}")
}

/// Builds the `Set-Cookie` header value carrying `token`. `SameSite=Strict`
/// keeps the cookie from being sent on cross-site navigations at all;
/// `Path=/` covers both the document routes and `/command`.
pub(crate) fn set_cookie_header(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE_NAME}={token}; Path=/; SameSite=Strict"))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == name).then_some(v)
    })
}

/// Verifies the double-submit token on a state-changing request. Returns
/// `true` (allowed) when no CSRF cookie is present at all — see module docs.
pub(crate) fn verify(headers: &HeaderMap) -> bool {
    match cookie_value(headers, COOKIE_NAME) {
        Some(cookie_token) => headers
            .get(HEADER_NAME)
            .and_then(|v| v.to_str().ok())
            .map(|header_token| header_token == cookie_token)
            .unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn no_cookie_is_allowed() {
        assert!(verify(&HeaderMap::new()));
    }

    #[test]
    fn matching_cookie_and_header_is_allowed() {
        let h = headers_with(&[
            ("cookie", "appfront_csrf=abc123; other=1"),
            ("x-csrf-token", "abc123"),
        ]);
        assert!(verify(&h));
    }

    #[test]
    fn mismatched_header_is_rejected() {
        let h = headers_with(&[
            ("cookie", "appfront_csrf=abc123"),
            ("x-csrf-token", "wrong"),
        ]);
        assert!(!verify(&h));
    }

    #[test]
    fn missing_header_is_rejected() {
        let h = headers_with(&[("cookie", "appfront_csrf=abc123")]);
        assert!(!verify(&h));
    }
}
