# Changelog

All notable changes to `tpt-appfront-server` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Per-peer-IP rate limiting on `POST /command` (`tower_governor` with
  `PeerIpKeyExtractor`), configurable through `SmartRouterBuilder::rate_limit` and
  `RateLimitConfig`.
- `SmartRouterBuilder::trust_forwarded_for`, which keys rate limits on
  `X-Forwarded-For` / `X-Real-Ip` / `Forwarded` when the app sits behind a trusted
  reverse proxy. Off by default.
- Separate rate limit for the read routes (`/`, `/ai-schema.json`, `/opengraph`,
  `/service-worker.js`, `/manifest.webmanifest`), configurable through
  `SmartRouterBuilder::read_rate_limit`.
- Double-submit-cookie CSRF check on `POST /command` (`SmartRouterBuilder::csrf`,
  on by default). The check applies when the router's CSRF cookie is present.
- `SmartRouterBuilder::auth_hook`, a predicate over request headers that rejects
  `POST /command` with `401` when it returns `false`.
- `SmartRouterBuilder::extra_ai_agents`, to add custom User-Agent substrings to AI
  agent detection. `client_kind::detect_with` takes the same list.
- Configurable `CorsPolicy` (`SmartRouterBuilder::cors`), defaulting to `Permissive`.
  `POST /command` never gets CORS headers.
- ETag / `If-None-Match` caching for the HTML, AI Schema JSON and OpenGraph responses.
- `serve()` runs the router with `into_make_service_with_connect_info::<SocketAddr>()`,
  so the per-peer rate limiter has an address to key on.
- PWA support: `service-worker.js` and `manifest.webmanifest` routes, and the manifest
  link and registration script added to the HTML shells when `SmartRouterBuilder::pwa`
  is set. `PwaConfig` also has optional Web Push (`push_vapid_public_key`), background
  sync (`background_sync_tag`) and an update prompt (`update_available_prompt`).
- Per-document `Content-Security-Policy` with a per-response nonce on the human shell,
  plus `X-Content-Type-Options: nosniff` and `X-Frame-Options: DENY` on every response.

### Changed
- **Breaking:** `serve()` returns `std::io::Result<()>` and reports bind and serve
  errors to the caller instead of panicking.
- Rate limiting on `POST /command` is per peer IP instead of one shared bucket for the
  whole route.

### Fixed
- The ETag caching helpers were defined but never called. Cached responses now go
  through them.

## [0.1.0]

### Added
- Initial release: Axum smart router (`SmartRouterBuilder`/`build_router`/`serve`)
  detecting client kind (browser / crawler / AI agent / social bot) and serving the
  matching backend; production middleware (`TraceLayer`, `TimeoutLayer`, security
  headers, `CorsLayer`); `POST /command` bridge (`Command`/`CommandResponse`) with
  body-size limit and `allowed_actions` allowlisting.
