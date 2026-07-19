# tpt-appfront-server

Axum smart router for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront): serves the right backend per client type, with PWA support and a `POST /command` bridge.

Classifies each request (human browser, crawler, AI agent, social bot) and serves the matching backend — the WASM shell for humans, `tpt-appfront-html` for crawlers/social bots, `tpt-appfront-ai-schema` for AI agents — layered with hardening middleware (tracing, timeouts, security headers, CORS) and an allowlist- and rate-limit-gated `POST /command` endpoint.

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-server = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and the [`ssr-page` example](https://github.com/tpt-solutions/tpt-appfront/tree/main/examples/ssr-page).

## License

Apache-2.0
