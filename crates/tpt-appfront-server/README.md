# tpt-appfront-server

The Axum **smart router** for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

A single Axum app serves the *same* `UITree` to four client kinds, detected via
`User-Agent` / query param (`?client=`):

- **Human browser** → the WASM/HTML shell (`tpt-appfront-dom`)
- **Crawler** → semantic HTML (`tpt-appfront-html`)
- **AI agent** → JSON-LD + custom AI Schema (`tpt-appfront-ai-schema`)
- **Social bot** → OpenGraph tags (`tpt-appfront-html`)

It also adds PWA/offline support and a bidirectional `POST /command` agent bridge.

## Features

- **`SmartRouterBuilder`** — configure the static dir, wasm path, title/
  description, PWA (`pwa(...)`), rate limiting, CSRF, and an `allowed_actions`
  allowlist for the command endpoint.
- **`build_router`** — returns the `axum::Router` for standalone `axum::serve`
  wiring (the e2e tests use this).
- **`serve`** — `into_make_service_with_connect_info::<SocketAddr>()` so per-peer
  rate limiting works.
- **`POST /command`** — `Command { action, params }` validated against the
  allowlist, body-size-limited (16 KiB), rate-limited per peer-IP, handed to an
  app-supplied `on_command` closure.
- **Production hardening** — `TraceLayer`, request `TimeoutLayer`, security
  headers (`X-Content-Type-Options`, `X-Frame-Options`, CSP), and a configurable
  `CorsPolicy`. (`CorsLayer` defaults to `Permissive` — tighten before exposing
  cross-origin.)
- **PWA** — `service-worker.js` + `manifest.webmanifest` + registration script.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-server = "0.1"
```

## Example

```rust
use tpt_appfront_server::{SmartRouterBuilder, Command, CommandResponse};

let router = SmartRouterBuilder::new()
    .title("My App")
    .wasm_path("/api/app.wasm")
    .allowed_actions(["increment".to_string()])
    .on_command(|cmd: Command| -> CommandResponse {
        // dispatch into your app's Msg / state here
        CommandResponse::ok()
    })
    .build(&ui, &agent_state);
```

In production, put a TLS terminator / reverse proxy in front — the router itself
is not a hardened edge.

## License

MIT OR Apache-2.0
