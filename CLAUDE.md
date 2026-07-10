# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT AppFront: write a UI once in Rust as an abstract `UITree<Msg>`, render it to multiple backends (native/WASM canvas via egui/wgpu, reactive DOM via web-sys, semantic HTML, AI/JSON-LD schema) from one codebase, and serve the right one per client via a smart router. Full design doc in [spec.txt](spec.txt); build checklist/phase status in [todo.md](todo.md) — check `todo.md` before assuming a feature exists, some phases (islands/partial hydration, webview desktop shell, GPU-compute layout) are still planned/stretch work.

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
   ├── appfront-html    — UITree -> semantic HTML string (SSR/SSG), data-ai-*/OpenGraph tags
   ├── appfront-ai-schema — UITree -> JSON-LD (schema.org) + custom AI Schema (interactive elements/actions/params)
   └── appfront-server  — Axum "smart router": detects ClientKind (browser/crawler/AI agent/social bot) and serves the matching backend
appfront-macros — #[appfront::component] proc macro (auto-fills meta.class/meta.ai.description); no static/dynamic codegen yet
appfront-cli    — `appfront` CLI: init/dev/build, scaffolds canvas/dom projects with path deps back into this checkout
```

- **`appfront-core`** (`crates/appfront-core/src`): `ui_tree.rs` defines `UITree<Msg>` (`kind: NodeKind<Msg>` + `meta: NodeMeta<Msg>` for `class`/`on_click`) and `ContainerBuilder`/`NodeRef`, the chainable builder API (`UITree::container(|c| { c.button("x").on_click(Msg::X); })`). The crate is generic over the app's own `Msg` enum — it has no opinion on what events exist. `signal.rs` is a from-scratch SolidJS-style reactive system: `Signal<T>::get()` subscribes the currently-running effect (tracked via a thread-local stack in `EFFECT_STACK`), `set()` re-runs only those effects, and dependencies are recomputed from scratch on every effect run so conditional branches re-subscribe correctly. `EffectHandle` must be kept alive (or `mem::forget`'d) or the effect stops firing when dropped.

- **`appfront-dom`** (`crates/appfront-dom/src/lib.rs`): gated behind `#![cfg(target_arch = "wasm32")]` — compiles to an empty crate on native so the workspace still builds. `mount()` walks the `UITree` once, building real DOM nodes directly (no virtual DOM/diffing); event closures are `.forget()`'d intentionally so they outlive the call (the DOM node itself is the only remaining owner). `reactive_text()` is the fine-grained-update primitive: it ties a single DOM text node directly to a `Signal<String>` via `create_effect`, bypassing the tree entirely on updates.

- **`appfront-canvas`** (`crates/appfront-canvas/src`): `CanvasApp` (`app.rs`) implements `eframe::App`; each frame it calls the app's `build_ui` closure to get a fresh `UITree` (immediate-mode, matching egui's own paradigm), builds a `taffy::TaffyTree` for layout (`layout.rs`), then paints (`paint.rs`) and dispatches any clicked `on_click` `Msg` through the `dispatch` callback. `run_native` (desktop) and `run_web` (mounts onto a `<canvas id="...">`, wasm32 only) are the two entry points exported from `lib.rs`. Text measurement is abstracted via `TextMeasurer` in `text.rs`.

- **`appfront-html`** (`crates/appfront-html/src/lib.rs`): `UITree` → semantic HTML string for SSR/SSG, including `data-ai-action` attributes and OpenGraph tags for social-bot crawls.

- **`appfront-ai-schema`** (`crates/appfront-ai-schema/src`): `json_ld.rs` renders `UITree` → JSON-LD (schema.org rich snippets); `ai_schema.rs` renders a custom AI-agent schema describing interactive elements/actions/params. Format frozen in [docs/ai-schema.md](docs/ai-schema.md).

- **`appfront-server`** (`crates/appfront-server/src`): `client_kind.rs` classifies a request (User-Agent/query param) into human/crawler/AI-agent/social-bot; `router.rs`'s `SmartRouter`/`SmartRouterBuilder` wires that classification to the right backend (WASM shell for humans, `appfront-html` for crawlers/social bots, `appfront-ai-schema` for AI agents) and exports `build_router` for standalone `axum::serve` use.

- **`appfront-macros`** (`crates/appfront-macros/src/lib.rs`): `#[appfront::component]` proc macro (re-exported as `appfront_core::component`) wraps a `UITree`-returning fn and auto-fills `meta.class` (kebab-cased fn name) and `meta.ai.description` (from the doc comment) on the root node if unset. Static/dynamic content analysis and codegen are not implemented yet (Phase 5).

- **`appfront-cli`** (`crates/appfront-cli/src`): `clap`-based `appfront init/dev/build`. `init` scaffolds a canvas/dom project with path deps back into this checkout; `dev --desktop`/`dev --web` shell out to `cargo run`/`trunk serve`; `build --target <canvas|dom|html|ai-schema>` shells out to `cargo build --release`/`trunk build --release` (or prints embedding guidance for the library-only `html`/`ai-schema` targets). See [docs/quickstart.md](docs/quickstart.md).

- **`examples/`**: excluded from the workspace (`Cargo.toml` `exclude = ["examples"]`) since they need their own dependency resolution (wasm-bindgen versions, `cdylib` crate-type for trunk). `counter-dom` builds with `trunk`; `counter-canvas` is a plain native `cargo run`. Both are built in CI's `examples` job. As of this writing the directory is empty in a fresh checkout — scaffold it locally with `appfront init` (see [docs/quickstart.md](docs/quickstart.md)) or restore the committed examples if/when they're added to the repo.

## Working in this repo

- Backends must stay independent consumers of `appfront-core` — don't add backend-specific fields to `UITree`/`NodeKind`/`NodeMeta`; extend the AST generically and let each backend interpret it.
- `appfront-dom` and the wasm side of `appfront-canvas` only compile under `wasm32-unknown-unknown`; if you touch either, build with `--target wasm32-unknown-unknown` too, not just native.
- Treat `todo.md` as the source of truth for what phase/feature is actually implemented vs. planned — `spec.txt` describes the eventual full design (including several features, like GPU-compute layout and the AutoOptimizer, that are explicitly stretch/future work).
