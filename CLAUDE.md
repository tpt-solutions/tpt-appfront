# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT AppFront: write a UI once in Rust as an abstract `UITree<Msg>`, render it to multiple backends (native/WASM canvas via egui/wgpu, reactive DOM via web-sys, semantic HTML, AI/JSON-LD schema) from one codebase. Full design doc in [spec.txt](spec.txt); build checklist/phase status in [todo.md](todo.md) — check `todo.md` before assuming a feature exists, most backends are still stubs.

## Commands

```sh
cargo build --workspace --all-targets     # native build, all crates
cargo test --workspace                    # run all tests
cargo test -p appfront-core signal::       # run one module's tests, e.g. signal.rs tests
cargo clippy --workspace --all-targets -- -D warnings   # lint (CI treats warnings as errors)

# WASM-only crates (appfront-dom, appfront-canvas support wasm32)
cargo build -p appfront-dom -p appfront-canvas --target wasm32-unknown-unknown
cargo clippy -p appfront-dom -p appfront-canvas --target wasm32-unknown-unknown --all-targets -- -D warnings

# examples (each has its own Cargo.toml, excluded from the workspace)
cd examples/counter-dom && trunk build   # or `trunk serve` to run in a browser
cd examples/counter-canvas && cargo run  # native winit/egui window
```

CI (`.github/workflows/ci.yml`) runs three jobs: `native` (build+test+clippy for the workspace), `wasm` (build+clippy for `appfront-dom`/`appfront-canvas` on `wasm32-unknown-unknown`), and `examples` (`trunk build` for every `examples/*/index.html`). Mirror these locally before pushing.

## Architecture

Everything flows through one abstract tree, defined once in `appfront-core`, consumed independently by each backend crate:

```
appfront-core (UITree<Msg> AST + Signal<T> reactive system)
   ├── appfront-dom     — wasm32-only; UITree -> real DOM via web-sys, no vdom
   ├── appfront-canvas  — native + wasm32; UITree -> egui via eframe, taffy layout
   ├── appfront-html    — stub; UITree -> semantic HTML string (SSR/SSG)
   └── appfront-ai-schema — stub; UITree -> JSON-LD / AI-agent schema
appfront-macros — stub; future #[appfront::component] compile-time optimizer
appfront-cli    — stub; future `appfront` CLI (init/dev/build)
```

- **`appfront-core`** (`crates/appfront-core/src`): `ui_tree.rs` defines `UITree<Msg>` (`kind: NodeKind<Msg>` + `meta: NodeMeta<Msg>` for `class`/`on_click`) and `ContainerBuilder`/`NodeRef`, the chainable builder API (`UITree::container(|c| { c.button("x").on_click(Msg::X); })`). The crate is generic over the app's own `Msg` enum — it has no opinion on what events exist. `signal.rs` is a from-scratch SolidJS-style reactive system: `Signal<T>::get()` subscribes the currently-running effect (tracked via a thread-local stack in `EFFECT_STACK`), `set()` re-runs only those effects, and dependencies are recomputed from scratch on every effect run so conditional branches re-subscribe correctly. `EffectHandle` must be kept alive (or `mem::forget`'d) or the effect stops firing when dropped.

- **`appfront-dom`** (`crates/appfront-dom/src/lib.rs`): gated behind `#![cfg(target_arch = "wasm32")]` — compiles to an empty crate on native so the workspace still builds. `mount()` walks the `UITree` once, building real DOM nodes directly (no virtual DOM/diffing); event closures are `.forget()`'d intentionally so they outlive the call (the DOM node itself is the only remaining owner). `reactive_text()` is the fine-grained-update primitive: it ties a single DOM text node directly to a `Signal<String>` via `create_effect`, bypassing the tree entirely on updates.

- **`appfront-canvas`** (`crates/appfront-canvas/src`): `CanvasApp` (`app.rs`) implements `eframe::App`; each frame it calls the app's `build_ui` closure to get a fresh `UITree` (immediate-mode, matching egui's own paradigm), builds a `taffy::TaffyTree` for layout (`layout.rs`), then paints (`paint.rs`) and dispatches any clicked `on_click` `Msg` through the `dispatch` callback. `run_native` (desktop) and `run_web` (mounts onto a `<canvas id="...">`, wasm32 only) are the two entry points exported from `lib.rs`. Text measurement is abstracted via `TextMeasurer` in `text.rs`.

- **`appfront-html`, `appfront-ai-schema`, `appfront-macros`, `appfront-cli`**: currently placeholder crates (default `cargo new` contents) — check `todo.md` Phases 5–8 before assuming any functionality exists here.

- **`examples/`**: excluded from the workspace (`Cargo.toml` `exclude = ["examples"]`) since they need their own dependency resolution (wasm-bindgen versions, `cdylib` crate-type for trunk). `counter-dom` builds with `trunk`; `counter-canvas` is a plain native `cargo run`. Both are built in CI's `examples` job.

## Working in this repo

- Backends must stay independent consumers of `appfront-core` — don't add backend-specific fields to `UITree`/`NodeKind`/`NodeMeta`; extend the AST generically and let each backend interpret it.
- `appfront-dom` and the wasm side of `appfront-canvas` only compile under `wasm32-unknown-unknown`; if you touch either, build with `--target wasm32-unknown-unknown` too, not just native.
- Treat `todo.md` as the source of truth for what phase/feature is actually implemented vs. planned — `spec.txt` describes the eventual full design (including several features, like GPU-compute layout and the AutoOptimizer, that are explicitly stretch/future work).
