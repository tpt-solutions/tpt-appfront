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
cd examples/counter-dom && trunk build    # or `trunk serve`
cd examples/counter-canvas && cargo run
```

## Architecture

```
appfront-core (UITree<Msg> AST + Signal<T> reactive system)
   ├── appfront-dom     — wasm32-only; UITree -> real DOM via web-sys, no vdom
   ├── appfront-canvas  — native + wasm32; UITree -> egui via eframe, taffy layout
   ├── appfront-html    — UITree -> semantic HTML string (SSR/SSG), data-ai-* attrs, OpenGraph
   ├── appfront-ai-schema — UITree -> JSON-LD (schema.org) + custom AI Schema (interactive elements/actions/params)
   └── appfront-server  — Axum smart router: ClientKind detection -> matching backend (DOM shell/HTML/AI-Schema)
appfront-macros — #[appfront::component] proc macro (auto-fills meta.class/meta.ai.description); no static/dynamic codegen yet
appfront-cli    — `appfront` CLI: init/dev/build
```

## Rules

- **Backends consume `appfront-core` only** — never add backend-specific fields to `UITree`/`NodeKind`/`NodeMeta`. Extend the AST generically.
- **`appfront-dom`** is `#![cfg(target_arch = "wasm32")]` — compiles to an empty crate on native (intentional, keeps workspace building). Touch it? Build for wasm too.
- **Signal effects**: `EffectHandle` must stay alive or the effect stops firing. Backends use `std::mem::forget` on event closures/effect handles intentionally — the DOM node/canvas is the only remaining owner.
- **`todo.md` is the source of truth** for what phase/feature exists vs planned. `spec.txt` describes the eventual full design (including stretch goals like GPU-compute layout). Don't assume unimplemented features exist.
- **examples/** is excluded from the workspace — each has its own `Cargo.toml`/dependency resolution. CI's examples job uses `trunk build` on every `examples/*/index.html`. Not present in a fresh checkout yet; scaffold locally with `appfront init`.
