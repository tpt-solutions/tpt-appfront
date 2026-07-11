//! Cross-origin policy for the smart router's read routes.

use axum::http::{HeaderValue, Method};
use tower_http::cors::CorsLayer;

/// Cross-origin policy applied to the read routes (`/`, `/ai-schema.json`,
/// `/opengraph`, `/service-worker.js`, `/manifest.webmanifest`). `POST
/// /command` never gets a `CorsLayer` regardless of this setting, so it can't
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

/// Builds the [`CorsLayer`] for the read routes from a [`CorsPolicy`].
pub(crate) fn cors_layer(policy: &CorsPolicy) -> CorsLayer {
    match policy {
        CorsPolicy::Permissive => CorsLayer::permissive(),
        CorsPolicy::Origins(origins) => {
            let values: Vec<HeaderValue> = origins
                .iter()
                .filter_map(|o| HeaderValue::from_str(o).ok())
                .collect();
            CorsLayer::new()
                .allow_origin(values)
                .allow_methods([Method::GET])
        }
    }
}
