# tpt-appfront-server

The **smart router** for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

One router serves the same `UITree` to four kinds of client, chosen from the
`User-Agent` header or the `?client=` query parameter:

| Client kind | Detected from | Served by |
|-------------|---------------|-----------|
| Human browser | default | WASM shell (`tpt-appfront-dom`), hydration-ready HTML when enabled |
| Crawler | Googlebot, Bingbot, YandexBot and similar | Semantic HTML (`tpt-appfront-html`) |
| AI agent | GPTBot, PerplexityBot, Claude and similar, plus `extra_ai_agents` | JSON-LD and AI Schema (`tpt-appfront-ai-schema`) |
| Social bot | Facebook, Slack, Twitter, Discord and similar | OpenGraph meta tags (`tpt-appfront-html`) |

The `?client=` values are `human`/`browser`/`web`, `crawler`/`bot`/`seo`,
`ai`/`agent`/`aiagent`, and `social`/`socialbot`/`opengraph`.

The router also serves `GET /ai-schema.json` and `GET /opengraph`, and exposes
`POST /command` so an AI agent can send actions to the app.

## Public API

Re-exported from `src/lib.rs`:

- `SmartRouterBuilder<Msg>`: `new(ui)`, then chain `title`, `description`,
  `static_dir`, `wasm_path`, `enable_hydration`, `signals`, `on_command`,
  `allowed_actions`, `rate_limit`, `read_rate_limit`, `csrf`, `auth_hook`, `pwa`,
  `cors`, `extra_ai_agents`, `trust_forwarded_for`; finish with `build()`, which
  returns a `SmartRouter<Msg>`.
- `build_router(SmartRouter<Msg>) -> Router<Msg>`, a request handler (`Router::handle`) for mounting the router yourself
  or stacking more middleware. The peer address comes from a `SocketAddr` request
  extension, which `serve` adds for every connection.
- `serve(SmartRouter<Msg>, SocketAddr) -> std::io::Result<()>` runs the router on a new
  listener. `serve_listener(SmartRouter<Msg>, tokio::net::TcpListener)` does the same on a
  listener you bound yourself.
- `Command { action: String, params: HashMap<String, serde_json::Value> }`, the body
  of `POST /command`.
- `CommandResponse { ok, message }`, with `CommandResponse::ok(..)` and
  `CommandResponse::err(..)`.
- `RateLimitConfig { per_second, burst }` and `CorsPolicy` (`Permissive` or
  `Origins(Vec<String>)`).
- `PwaConfig`, which has a `Default` implementation.
- `detect(ua, query_client)` and `detect_with(ua, query_client, extra_agents)`, which
  return a `ClientKind`. `ClientKind` has the variants `Human`, `Crawler`, `AiAgent`
  and `SocialBot`.
- PWA helpers: `service_worker`, `manifest`, `manifest_link`, `registration_script`
  and `update_available_script`.

## Command endpoint

`POST /command` takes a JSON `Command`:

- It returns `501` when no `on_command` handler is set.
- It returns `400` for an empty `action`, and `403` when `allowed_actions` is set and
  the action is not in it.
- It returns `401` when `auth_hook` returns `false`.
- Requests that carry the router's CSRF cookie must also send the matching token
  header. Requests without the cookie are not checked, so direct API clients still
  work.
- Bodies are limited to 16 KiB, and requests are rate-limited per peer IP.
- The route has no CORS headers, so other sites cannot call it from a browser.

Without `allowed_actions`, every action the handler accepts is allowed.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-server = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

This crate has no Cargo feature flags.

## Example

```rust
use std::net::SocketAddr;
use tpt_appfront_server::{serve, Command, CommandResponse, SmartRouterBuilder};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let ui = build_ui(); // your app's UITree<Msg>

    let router = SmartRouterBuilder::new(ui)
        .title("My App")
        .description("A demo app")
        .wasm_path("/app.wasm")
        .allowed_actions(["increment"])
        .on_command(|cmd: Command| match cmd.action.as_str() {
            "increment" => CommandResponse::ok("incremented"),
            _ => CommandResponse::err("unknown action"),
        })
        .build();

    serve(router, "127.0.0.1:3000".parse::<SocketAddr>().unwrap()).await
}
```

`Msg` must be `Clone + Send + Sync + serde::Serialize + 'static` to use `serve` or
`build_router`.

The `on_command` handler is where you call `tpt_appfront_core::trigger_event` or
`navigate_to` against your app's state.

## Production notes

The router is not a hardened edge. Put a TLS terminator or reverse proxy in front of
it. The default CORS policy is `Permissive` for the read routes, so set
`SmartRouterBuilder::cors(CorsPolicy::Origins(..))` if other origins should not read
them. Only enable `trust_forwarded_for` behind a proxy that sets those headers, or
clients can pick their own rate-limit key.

## License

MIT OR Apache-2.0
