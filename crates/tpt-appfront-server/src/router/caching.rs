//! Response caching for the deterministic read routes (`crawler_html`/
//! `opengraph`/`ai-schema.json`): the underlying `UITree` is fixed once the
//! `SmartRouter` is built, so these routes render once (memoized in a
//! `OnceLock` on the router state) and serve the cached body + `ETag`
//! afterward — honoring `If-None-Match` with a bare `304` — instead of
//! re-rendering (and re-serializing) on every request.

use std::sync::OnceLock;

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Json, Response};

fn etag_for(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("\"{:016x}\"", hasher.finish())
}

fn not_modified(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').any(|tag| tag.trim() == etag))
        .unwrap_or(false)
}

const CACHE_CONTROL_VALUE: &str = "public, max-age=60, must-revalidate";

/// Serves a cached HTML body, computing it on first call via `render`.
pub(crate) fn cached_html<F: FnOnce() -> String>(
    cell: &OnceLock<(String, String)>,
    headers: &HeaderMap,
    render: F,
) -> Response {
    let (body, etag) = cell.get_or_init(|| {
        let body = render();
        let etag = etag_for(&body);
        (body, etag)
    });

    if not_modified(headers, etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }

    (
        StatusCode::OK,
        [
            (
                axum::http::header::ETAG,
                HeaderValue::from_str(etag).unwrap_or_else(|_| HeaderValue::from_static("")),
            ),
            (
                axum::http::header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL_VALUE),
            ),
        ],
        Html(body.clone()),
    )
        .into_response()
}

/// Serves a cached JSON body, computing it on first call via `render`.
pub(crate) fn cached_json<F: FnOnce() -> serde_json::Value>(
    cell: &OnceLock<(serde_json::Value, String)>,
    headers: &HeaderMap,
    render: F,
) -> Response {
    let (body, etag) = cell.get_or_init(|| {
        let body = render();
        let etag = etag_for(&body.to_string());
        (body, etag)
    });

    if not_modified(headers, etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }

    (
        StatusCode::OK,
        [
            (
                axum::http::header::ETAG,
                HeaderValue::from_str(etag).unwrap_or_else(|_| HeaderValue::from_static("")),
            ),
            (
                axum::http::header::CACHE_CONTROL,
                HeaderValue::from_static(CACHE_CONTROL_VALUE),
            ),
        ],
        Json(body.clone()),
    )
        .into_response()
}
