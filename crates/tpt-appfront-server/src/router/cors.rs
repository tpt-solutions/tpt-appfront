//! Cross-origin policy for the smart router's read routes.

use http::{header, HeaderMap, HeaderValue, StatusCode};

use crate::app::{bare, Resp};

/// Cross-origin policy applied to the read routes (`/`, `/ai-schema.json`,
/// `/opengraph`, `/service-worker.js`, `/manifest.webmanifest`). `POST
/// /command` never gets CORS headers regardless of this setting, so it can't
/// be driven cross-site.
#[derive(Debug, Clone, Default)]
pub enum CorsPolicy {
    /// Any origin may read the public routes. This is safe for these routes
    /// specifically because they're unauthenticated GET-only content (HTML/
    /// JSON-LD/AI-Schema meant to be crawled/fetched by anyone), but it's
    /// wider than most deployments need.
    #[default]
    Permissive,
    /// Only the listed origins (e.g. `"https://example.com"`) may read the
    /// public routes cross-origin. Same-origin requests are always allowed
    /// regardless of this list.
    Origins(Vec<String>),
}

const ALLOW_METHODS: &str = "GET, HEAD, OPTIONS";

/// Adds the CORS headers for `policy` to a read-route response.
pub(crate) fn apply(policy: &CorsPolicy, request: &HeaderMap, resp: &mut Resp) {
    let headers = resp.headers_mut();
    match policy {
        CorsPolicy::Permissive => {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            );
        }
        CorsPolicy::Origins(origins) => {
            // The answer depends on the Origin header, so caches must key on it.
            headers.append(header::VARY, HeaderValue::from_static("origin"));
            if let Some(origin) = request
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .filter(|o| origins.iter().any(|allowed| allowed == o))
            {
                if let Ok(value) = HeaderValue::from_str(origin) {
                    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
                }
            }
        }
    }
}

/// Answers a CORS preflight (`OPTIONS`) on a read route.
pub(crate) fn preflight(policy: &CorsPolicy, request: &HeaderMap) -> Resp {
    let mut resp = bare(StatusCode::OK);
    apply(policy, request, &mut resp);
    let headers = resp.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(ALLOW_METHODS),
    );
    if let Some(requested) = request.get(header::ACCESS_CONTROL_REQUEST_HEADERS) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, requested.clone());
    }
    resp
}
