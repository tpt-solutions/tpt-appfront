# Changelog

All notable changes to `tpt-appfront-server` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Per-peer-IP rate limiting on `POST /command` (`PeerIpKeyExtractor` + `governor`).
- Configurable `CorsPolicy` (defaults to `Permissive`).
- ETag / `If-None-Match` caching wired through (`cached_html`/`cached_json`/
  `opengraph_cache`).
- `serve()` over `into_make_service_with_connect_info::<SocketAddr>()` so
  `ConnectInfo` is populated for peer-aware middleware.
- PWA support: `service-worker.js`, `manifest.webmanifest`, and registration script
  spliced into the WASM/hydration shells when `pwa(...)` is configured.

## [0.1.0]

### Added
- Initial release: Axum smart router (`SmartRouterBuilder`/`build_router`/`serve`)
  detecting client kind (browser / crawler / AI agent / social bot) and serving the
  matching backend; production middleware (`TraceLayer`, `TimeoutLayer`, security
  headers, `CorsLayer`); `POST /command` bridge (`Command`/`CommandResponse`) with
  body-size limit and `allowed_actions` allowlisting.
