# TPT AppFront

A unified, hardware-accelerated, AI-native UI framework: write your UI once in Rust as an abstract `UITree`, render it to native canvas, a fine-grained-reactive DOM, semantic HTML, machine-readable AI/JSON-LD schemas, a terminal UI, or an OS-webview desktop shell — from one codebase.

See [spec.txt](spec.txt) for the full design document and [todo.md](todo.md) for build progress (most backends are functional but not every stretch item is complete — check there before assuming a feature exists).

## Quickstart

See [docs/quickstart.md](docs/quickstart.md): install the CLI, `appfront init`, `appfront dev`, `appfront build`.

## Workspace layout

- `crates/appfront-core` — `UITree` AST, the reactive `Signal` system, virtual scroll, styling utilities, devtools inspector, static-tree caching
- `crates/appfront-dom` — fine-grained-reactive real DOM backend (web-sys), wasm32-only
- `crates/appfront-canvas` — egui/taffy hardware-accelerated canvas backend (glow renderer), native + wasm32
- `crates/appfront-html` — semantic HTML (SSR/SSG) backend for crawlers, with `data-ai-*`/OpenGraph tags
- `crates/appfront-ai-schema` — JSON-LD (schema.org) and custom AI Schema output backend
- `crates/appfront-server` — Axum "smart router" that serves the right backend (DOM/HTML/AI-Schema) per client type (browser, crawler, AI agent, social bot), plus PWA support and a `POST /command` bridge
- `crates/appfront-tui` — terminal UI backend (ratatui/crossterm), keyboard-driven
- `crates/appfront-webview` — thin OS-webview desktop shell (wry + tao) hosting the `appfront-dom` WASM bundle, no Electron/Node required
- `crates/appfront-mcp` — MCP server exposing the programmatic AI-agent API as JSON-RPC tools over stdio
- `crates/appfront-macros` — `#[appfront::component]` helper macro and the `view!`/`rsx!` templating macro
- `crates/appfront-cli` — the `appfront` CLI (`init`/`dev`/`build`/`generate`/`benchmark`/`optimize`)

Examples (`examples/counter-dom`, `counter-canvas`, `counter-tui`, `counter-webview`, `ssr-page`, `ai-agent-demo`) live outside the workspace — each has its own `Cargo.toml` so it can resolve wasm-bindgen/trunk dependencies independently. All are committed and built in CI (see `.github/workflows/ci.yml`); scaffold new ones locally with `appfront init` (see [docs/quickstart.md](docs/quickstart.md)).

## License

Apache-2.0
