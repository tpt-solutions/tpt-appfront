# TPT AppFront

[![CI](https://github.com/tpt-solutions/tpt-appfront/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-appfront/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tpt-appfront-core.svg)](https://crates.io/crates/tpt-appfront-core)
[![docs.rs](https://img.shields.io/docsrs/tpt-appfront-core)](https://docs.rs/tpt-appfront-core)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-APACHE)

A unified, hardware-accelerated, AI-native UI framework: write your UI once in Rust as an abstract `UITree`, render it to native canvas, a fine-grained-reactive DOM, semantic HTML, machine-readable AI/JSON-LD schemas, a terminal UI, or an OS-webview desktop shell — from one codebase.

See [spec.txt](spec.txt) for the full design document and [todo.md](todo.md) for build progress (most backends are functional but not every stretch item is complete — check there before assuming a feature exists).

## Quickstart

See [docs/quickstart.md](docs/quickstart.md): install the CLI, `tpt-appfront init`, `tpt-appfront dev`, `tpt-appfront build`.

## Workspace layout

- `crates/tpt-appfront-core` — `UITree` AST, the reactive `Signal` system, virtual scroll, styling utilities, devtools inspector, static-tree caching
- `crates/tpt-appfront-dom` — fine-grained-reactive real DOM backend (web-sys), wasm32-only
- `crates/tpt-appfront-canvas` — egui/taffy hardware-accelerated canvas backend (glow renderer), native + wasm32
- `crates/tpt-appfront-html` — semantic HTML (SSR/SSG) backend for crawlers, with `data-ai-*`/OpenGraph tags
- `crates/tpt-appfront-ai-schema` — JSON-LD (schema.org) and custom AI Schema output backend
- `crates/tpt-appfront-server` — Axum "smart router" that serves the right backend (DOM/HTML/AI-Schema) per client type (browser, crawler, AI agent, social bot), plus PWA support and a `POST /command` bridge
- `crates/tpt-appfront-tui` — terminal UI backend (ratatui/crossterm), keyboard-driven
- `crates/tpt-appfront-webview` — thin OS-webview desktop shell (wry + tao) hosting the `tpt-appfront-dom` WASM bundle, no Electron/Node required
- `crates/tpt-appfront-mcp` — MCP server exposing the programmatic AI-agent API as JSON-RPC tools over stdio
- `crates/tpt-appfront-macros` — `#[appfront::component]` helper macro and the `view!`/`rsx!` templating macro
- `crates/tpt-appfront-cli` — the `tpt-appfront` CLI (`init`/`dev`/`build`/`generate`/`ingest`/`benchmark`/`optimize`/`doctor`/`add`)
- `crates/tpt-appfront-templates` — backend-agnostic starter UI templates (`login_form`, `dashboard_shell`, `settings_list`) as stateless `UITree` builder functions, usable from any backend

Examples (`examples/counter-dom`, `counter-canvas`, `counter-tui`, `counter-webview`, `ssr-page`, `ai-agent-demo`, `todo-app`, `templates-demo`, `node-graph`) live outside the workspace — each has its own `Cargo.toml` so it can resolve wasm-bindgen/trunk dependencies independently. All are committed and built in CI (see `.github/workflows/ci.yml`); scaffold new ones locally with `tpt-appfront init` (see [docs/quickstart.md](docs/quickstart.md)). `templates-demo` composes the `tpt-appfront-templates` starter UIs; `node-graph` shows the recommended raw-`egui` escape hatch for GPU/canvas-style interactions the `UITree` flexbox can't express.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
