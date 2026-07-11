# TPT AppFront — Agent Guide

## Commands

```sh
cargo build --workspace --all-targets     # native
cargo test --workspace                    # all tests
cargo clippy --workspace --all-targets -- -D warnings

# WASM-only crates need explicit target
cargo build -p appfront-dom -p appfront-canvas --target wasm32-unknown-unknown
cargo clippy -p appfront-dom -p appfront-canvas --target wasm32-unknown-unknown --all-targets -- -D warnings

# Examples have their own Cargo.toml (excluded from workspace)
cd examples/counter-dom && trunk build     # or `trunk serve`
cd examples/counter-canvas && cargo run
cd examples/counter-tui && cargo run
cd examples/counter-webview && cargo run   # needs a pre-built ui/dist
```

CI also runs `cargo audit` and `cargo deny check` (root `deny.toml`) — see `.github/workflows/ci.yml`.

## Architecture

```
appfront-core (UITree<Msg> AST + Signal<T> reactive system + virtual scroll + styling + devtools + static-tree caching)
   ├── appfront-dom     — wasm32-only; UITree -> real DOM via web-sys, keyed diffing, hydration, no vdom
   ├── appfront-canvas  — native + wasm32; UITree -> egui via eframe (glow renderer), taffy layout
   ├── appfront-html    — UITree -> semantic HTML string (SSR/SSG), data-ai-* attrs, OpenGraph
   ├── appfront-ai-schema — UITree -> JSON-LD (schema.org) + custom AI Schema (interactive elements/actions/params)
   ├── appfront-server  — Axum smart router: ClientKind detection -> matching backend, PWA support, POST /command
   ├── appfront-tui     — UITree -> ratatui terminal widgets, keyboard-driven focus/dispatch
   ├── appfront-webview — native window (wry + tao) hosting the appfront-dom WASM bundle, native<->JS IPC
   └── appfront-mcp     — MCP server exposing query_state/navigate/per-node actions as JSON-RPC tools over stdio
appfront-macros — #[appfront::component] proc macro + view!/rsx! templating macro with static-subtree hoisting
appfront-cli    — `appfront` CLI: init/dev/build/generate/benchmark/optimize
```

## Rules

- **Backends consume `appfront-core` only** — never add backend-specific fields to `UITree`/`NodeKind`/`NodeMeta`. Extend the AST generically.
- **`appfront-dom`** is `#![cfg(target_arch = "wasm32")]` — compiles to an empty crate on native (intentional, keeps workspace building). Touch it? Build for wasm too.
- **Signal effects**: `EffectHandle` must stay alive or the effect stops firing. Backends use `std::mem::forget` on event closures/effect handles intentionally — the DOM node/canvas is the only remaining owner.
- **`todo.md` is the source of truth** for what phase/feature exists vs planned. `spec.txt` describes the eventual full design (including stretch goals like GPU-compute layout). Don't assume unimplemented features exist.
- **examples/** is excluded from the workspace — each has its own `Cargo.toml`/dependency resolution. All six examples (`counter-dom`, `counter-canvas`, `counter-tui`, `counter-webview`, `ssr-page`, `ai-agent-demo`) are committed and built in CI; `counter-webview` is skipped by the CI examples job (needs system webview libs + a pre-built `ui/dist`) and should be verified locally.
