# TPT AppFront

A unified, hardware-accelerated, AI-native UI framework: write your UI once in Rust as an abstract `UITree`, render it to native canvas, a fine-grained-reactive DOM, semantic HTML, or machine-readable AI/JSON-LD schemas — from one codebase.

See [spec.txt](spec.txt) for the full design document and [todo.md](todo.md) for build progress (most backends are functional but not all phases are complete — check there before assuming a feature exists).

## Quickstart

See [docs/quickstart.md](docs/quickstart.md): install the CLI, `appfront init`, `appfront dev`, `appfront build`.

## Workspace layout

- `crates/appfront-core` — `UITree` AST and the reactive `Signal` system
- `crates/appfront-dom` — fine-grained-reactive real DOM backend (web-sys), wasm32-only
- `crates/appfront-canvas` — wgpu/egui hardware-accelerated canvas backend, native + wasm32
- `crates/appfront-html` — semantic HTML (SSR/SSG) backend for crawlers, with `data-ai-*`/OpenGraph tags
- `crates/appfront-ai-schema` — JSON-LD (schema.org) and custom AI Schema output backend
- `crates/appfront-server` — Axum "smart router" that serves the right backend (DOM/HTML/AI-Schema) per client type (browser, crawler, AI agent, social bot)
- `crates/appfront-macros` — `#[appfront::component]` compile-time helper macro
- `crates/appfront-cli` — the `appfront` CLI (`init`/`dev`/`build`)

Examples (`examples/counter-dom`, `examples/counter-canvas`) live outside the workspace — each has its own `Cargo.toml` so it can resolve wasm-bindgen/trunk dependencies independently. Not present in a fresh checkout yet; scaffold one locally with `appfront init` (see [docs/quickstart.md](docs/quickstart.md)).

## License

Apache-2.0
