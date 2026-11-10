//! Response caching for the deterministic read routes (`crawler_html`/
//! `opengraph`/`ai-schema.json`): the underlying `UITree` is fixed once the
//! `SmartRouter` is built, so these routes render once (memoized in a
//! `OnceLock` on the router state) and serve the cached body + `ETag`
//! afterward — honoring `If-None-Match` with a bare `304` — instead of
//! re-rendering (and re-serializing) on every request.

use std::sync::OnceLock;

use http::{header, HeaderMap, HeaderValue, StatusCode};

use crate::app::{bare, html, json, Resp};

fn etag_for(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("\"{:016x}\"", hasher.finish())
}

fn not_modified(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').any(|tag| tag.trim() == etag))
        .unwrap_or(false)
}

const CACHE_CONTROL_VALUE: &str = "public, max-age=60, must-revalidate";

fn with_cache_headers(mut resp: Resp, etag: &str) -> Resp {
    let headers = resp.headers_mut();
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(etag).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_VALUE),
    );
    resp
}

/// Serves a cached HTML body, computing it on first call via `render`.
pub(crate) fn cached_html<F: FnOnce() -> String>(
    cell: &OnceLock<(String, String)>,
    headers: &HeaderMap,
    render: F,
) -> Resp {
    let (body, etag) = cell.get_or_init(|| {
        let body = render();
        let etag = etag_for(&body);
        (body, etag)
    });

    if not_modified(headers, etag) {
        return with_cache_headers(bare(StatusCode::NOT_MODIFIED), etag);
    }
    with_cache_headers(html(body.clone()), etag)
}

/// Serves a cached JSON body, computing it on first call via `render`.
pub(crate) fn cached_json<F: FnOnce() -> serde_json::Value>(
    cell: &OnceLock<(serde_json::Value, String)>,
    headers: &HeaderMap,
    render: F,
) -> Resp {
    let (body, etag) = cell.get_or_init(|| {
        let body = render();
        let etag = etag_for(&body.to_string());
        (body, etag)
    });

    if not_modified(headers, etag) {
        return with_cache_headers(bare(StatusCode::NOT_MODIFIED), etag);
    }
    with_cache_headers(json(StatusCode::OK, body), etag)
}
