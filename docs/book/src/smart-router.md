# Smart router

`tpt-appfront-server` serves one UI in several forms and picks one for each request. The client kind
comes from the User-Agent or a query parameter:

- **Browser:** the DOM build (WebAssembly).
- **Crawler:** semantic HTML.
- **AI agent:** the AI schema.
- **Social bot:** HTML with OpenGraph tags.

```rust,ignore
use tpt_appfront_server::{build_router, SmartRouterBuilder};

let router = SmartRouterBuilder::new(home_ui())
    .title("Shop")
    .description("Handmade tea")
    .static_dir("dist")
    .wasm_path("/app.wasm")
    .allowed_actions(["add_to_cart"])
    .on_command(|cmd| handle_command(cmd))
    .build();

let app = build_router(router);
```

## Options

- `title` and `description` set the page metadata.
- `static_dir` and `wasm_path` locate the browser build.
- `allowed_actions` lists the only actions `POST /command` accepts. With nothing listed, none are accepted.
- `on_command` handles accepted actions. Without it, the route answers `501`.
- `rate_limit` and `read_rate_limit` set per-client limits.
- `csrf` and `cors` protect the command route and cross-origin reads.
- `pwa` adds a service worker and a web app manifest.
- `trust_forwarded_for` takes the client address from `X-Forwarded-For`. Enable it only behind a proxy
  you control.
- `auth_hook` runs a check before a request is served.
