# appfront-server

Axum smart router serving the right [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) backend per client.

`client_kind.rs` classifies each request (by User-Agent and an optional query-param override) into human/crawler/AI-agent/social-bot. `SmartRouter`/`SmartRouterBuilder` wire that classification to the matching backend: the WASM shell for humans, `appfront-html` for crawlers and social bots, and `appfront-ai-schema` for AI agents. The router layers on hardening middleware (`TraceLayer`/`TimeoutLayer`, security headers, CORS), exposes a body-size-limited, allowlist- and rate-limit-gated `POST /command` endpoint that dispatches to an app-supplied `on_command` closure, and optionally serves a PWA service worker + web app manifest via `SmartRouterBuilder::pwa(..)`.

`build_router` is exported for standalone `axum::serve` use if you don't need the full `SmartRouter` wiring.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture.
