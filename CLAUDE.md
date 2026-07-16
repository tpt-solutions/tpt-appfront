# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT AppFront: write a UI once in Rust as an abstract `UITree<Msg>`, render it to multiple backends (native/WASM canvas via egui/taffy, reactive DOM via web-sys, semantic HTML, AI/JSON-LD schema, a terminal UI via ratatui, and an OS-webview desktop shell) from one codebase, and serve the right one per client via a smart router. Full design doc in [spec.txt](spec.txt); build checklist/phase status in [todo.md](todo.md) — check `todo.md` before assuming a feature exists, a few items (GPU-compute layout, mobile webview, binary-size-vs-Tauri measurement) are still stretch/unfinished work.

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
cd examples/counter-dom && trunk build     # or `trunk serve` to run in a browser
cd examples/counter-canvas && cargo run    # native winit/egui window
cd examples/counter-tui && cargo run       # terminal UI (ratatui/crossterm)
cd examples/counter-webview && cargo run   # OS-webview desktop shell (needs a pre-built ui/dist)
cd examples/ssr-page && cargo run          # prints a semantic HTML string
cd examples/ai-agent-demo && cargo run     # headless query_state/navigate_to/trigger_event demo
```

CI (`.github/workflows/ci.yml`) runs five jobs: `native` (build+test+clippy for the workspace), `wasm` (build+clippy for `appfront-dom`/`appfront-canvas` on `wasm32-unknown-unknown`), `audit` (`cargo audit`), `deny` (`cargo deny check` against root `deny.toml`), and `examples` (`trunk build` for every `examples/*/index.html`, plus a native `cargo build` pass for non-trunk examples — the webview example is deliberately skipped there since it needs system webview libs and a pre-built `ui/dist`). Mirror these locally before pushing.

## Architecture

Everything flows through one abstract tree, defined once in `appfront-core`, consumed independently by each backend crate:

```
appfront-core (UITree<Msg> AST + Signal<T> reactive system + virtual scroll + styling + devtools + static-tree caching)
   ├── appfront-dom     — wasm32-only; UITree -> real DOM via web-sys, keyed list diffing, hydration, no vdom
   ├── appfront-canvas  — native + wasm32; UITree -> egui via eframe (glow renderer), taffy layout, AutoOptimizer
   ├── appfront-html    — UITree -> semantic HTML string (SSR/SSG), data-ai-*/OpenGraph tags
   ├── appfront-ai-schema — UITree -> JSON-LD (schema.org) + custom AI Schema (interactive elements/actions/params)
   ├── appfront-server  — Axum "smart router": detects ClientKind (browser/crawler/AI agent/social bot), serves the matching backend, PWA/service-worker support, POST /command endpoint
   ├── appfront-tui     — UITree -> ratatui terminal widgets, keyboard-driven focus/dispatch
   ├── appfront-webview — thin native window (wry + tao) hosting the appfront-dom WASM bundle, native<->JS IPC
   └── appfront-mcp     — MCP server exposing query_state/navigate/per-node actions as JSON-RPC tools over stdio
appfront-macros — #[appfront::component] proc macro (auto-fills meta.class/meta.ai.description) + view!/rsx! templating macro with static-subtree hoisting
appfront-cli    — `appfront` CLI: init/dev/build/generate/benchmark/optimize, scaffolds canvas/dom/tui/webview projects with path deps back into this checkout
```

- **`appfront-core`** (`crates/appfront-core/src`): `ui_tree.rs` defines `UITree<Msg>` (`kind: NodeKind<Msg>` + `meta: NodeMeta<Msg>` for `class`/`on_click`/`key`/`virtual_scroll`) and `ContainerBuilder`/`NodeRef`, the chainable builder API (`UITree::container(|c| { c.button("x").on_click(Msg::X); })`). The crate is generic over the app's own `Msg` enum — it has no opinion on what events exist. `signal.rs` is a from-scratch SolidJS-style reactive system: `Signal<T>::get()` subscribes the currently-running effect (tracked via a thread-local stack in `EFFECT_STACK`), `set()` re-runs only those effects, dependencies are recomputed from scratch on every effect run so conditional branches re-subscribe correctly, and `create_memo`/`batch()` add memoized derived signals and rank-ordered diamond-dependency-safe batched updates. `EffectHandle` must be kept alive (or `mem::forget`'d) or the effect stops firing when dropped. Also home to `virtual_scroll.rs` (windowed list rendering primitive), `styling.rs` + the `class!` macro (curated Tailwind-like utility classes, compile-time unknown-class checking), `static_tree.rs` (cache for provably-static subtrees hoisted by `view!`), `reconcile.rs` (backend-agnostic keyed-diff primitive), and `devtools.rs` (plain-text/HTML tree + agent-state + signal-activity inspector).

- **`appfront-dom`** (`crates/appfront-dom/src/lib.rs`): gated behind `#![cfg(target_arch = "wasm32")]` — compiles to an empty crate on native so the workspace still builds. `mount()` returns a `MountedRoot<Msg>` whose `render()` reconciles the mounted subtree against a new `UITree` *in place* (reusing `reconcile::reconcile_keys` for keyed `List` children, positional diff for `Container`/`DataGrid`), and whose `unmount()` clears every event listener, drops every `EffectHandle`, and releases every tracked `Closure` (recorded in `LIVE_CLOSURES`) — no leaks. `render(view) -> EffectHandle` is the non-router fine-grained entry point (re-render only changed subtrees on signal change). `mount_router` reconciles rather than full-replacing on navigation. Event closures are tracked (not `.forget()`-leaked) so `unmount` can release them. `reactive_text()` is the fine-grained-update primitive, ties a DOM text node directly to a `Signal<String>` via `create_effect`, and batches updates into one `requestAnimationFrame` flush per frame. `update_list` diffs keyed `List`/`DataGrid` children instead of rebuilding wholesale, and drives `VirtualScroll` windowing when set. `hydrate_node` implements islands/partial hydration: only subtrees with listeners/actions or flagged `is_dynamic` get listeners attached, inert static content stays untouched.

- **`appfront-canvas`** (`crates/appfront-canvas/src`): `CanvasApp` (`app.rs`) implements `eframe::App`; each frame it calls the app's `build_ui` closure to get a fresh `UITree` (immediate-mode, matching egui's own paradigm), builds a `taffy::TaffyTree` for layout (`layout.rs`), then paints (`paint.rs`) and dispatches any clicked `on_click` `Msg` through the `dispatch` callback. Uses `eframe`'s `glow` (GL/GLES) renderer, not `wgpu` (deliberately dropped for binary size/dep-tree reasons — see todo.md Phase 4b). `run_native` (desktop) and `run_web` (mounts onto a `<canvas id="...">`, wasm32 only) are the two entry points exported from `lib.rs`. Text measurement is abstracted via `TextMeasurer` in `text.rs` (heuristic estimator by default; `full-text-shaping` feature enables `cosmic-text`, with a bundled Noto Sans font for wasm). `auto_optimizer.rs` profiles per-frame timing and recommends toggling virtual-scrolling/texture-caching (recommendation logic only — canvas doesn't yet act on it). Optional `accesskit` feature wires screen-reader names/roles into painted widgets.

- **`appfront-html`** (`crates/appfront-html/src/lib.rs`): `UITree` → semantic HTML string for SSR/SSG, including `data-ai-action` attributes, OpenGraph tags for social-bot crawls, and inline `style` attributes from `appfront-core::styling`.

- **`appfront-ai-schema`** (`crates/appfront-ai-schema/src`): `json_ld.rs` renders `UITree` → JSON-LD (schema.org rich snippets); `ai_schema.rs` renders a custom AI-agent schema describing interactive elements/actions/params. Format frozen in [docs/ai-schema.md](docs/ai-schema.md).

- **`appfront-server`** (`crates/appfront-server/src`): `client_kind.rs` classifies a request (User-Agent/query param) into human/crawler/AI-agent/social-bot; `router.rs`'s `SmartRouter`/`SmartRouterBuilder` wires that classification to the right backend (WASM shell for humans, `appfront-html` for crawlers/social bots, `appfront-ai-schema` for AI agents), layers hardening middleware (`TraceLayer`/`TimeoutLayer`/security headers/CORS), exposes `POST /command` (body-size-limited, allowlist- and rate-limit-gated, dispatches to an app-supplied `on_command` closure), and exports `build_router` for standalone `axum::serve` use. `pwa.rs` generates a service worker + web app manifest and wires `GET /service-worker.js` / `GET /manifest.webmanifest` when `SmartRouterBuilder::pwa(..)` is configured.

- **`appfront-tui`** (`crates/appfront-tui/src`): `NodeKind` → `ratatui` widgets (`Container`→layout split, `Text`/`Heading`→paragraph, `Button`→focusable bracketed paragraph, `Input`→editable line, `List`→list, `DataGrid`→table). Pure/headless-testable via `ratatui`'s `TestBackend`. `TuiDriver` handles keyboard-driven focus/dispatch (Tab/arrows/Enter/Space/Esc/typing) independent of any real TTY.

- **`appfront-webview`** (`crates/appfront-webview/src/lib.rs`): thin native window via `wry` + `tao`, no bundled renderer — serves a `trunk`-built `dist/` directory (the same `appfront-dom` WASM app used on the web) over an `app://` custom protocol. `WebviewOptions::allowed_actions` allowlists which IPC actions a hosted page may dispatch back to native, and `max_commands_per_second` rate-limits the IPC bridge.

- **`appfront-mcp`** (`crates/appfront-mcp/src/lib.rs`): `McpServer<Msg>` auto-generates one MCP tool per interactive `UITree` node's `AiMeta` (plus built-in `query_state`/`navigate` tools), wired to `appfront_core::query_state`/`navigate_to` and an app-supplied `on_command` closure. Speaks JSON-RPC 2.0 newline-delimited over stdio.

- **`appfront-macros`** (`crates/appfront-macros/src/lib.rs`): `#[appfront::component]` proc macro (re-exported as `appfront_core::component`) wraps a `UITree`-returning fn and auto-fills `meta.class` (kebab-cased fn name) and `meta.ai.description` (from the doc comment) on the root node if unset, using a token-level heuristic to flag static/dynamic. `view.rs` implements `view!`/`rsx!` (re-exported as `appfront_core::view`), an HTML-like template macro covering `Container`/`Heading`/`Text`/`Button`/`Input` (not `List`/`DataGrid` or two-way binding yet) that precisely detects provably-static subtrees and hoists them into `appfront_core::static_tree`'s build-once cache instead of rebuilding every render.

- **`appfront-cli`** (`crates/appfront-cli/src`): `clap`-based `appfront init/dev/build/generate/benchmark/optimize`. `init` scaffolds a canvas/dom/tui/webview project with path deps back into this checkout; `dev --desktop`/`--web`/`--tui`/`--desktop-webview` shell out to `cargo run`/`trunk serve`; `build --target <canvas|dom|tui|webview|html|ai-schema>` shells out to `cargo build --release`/`trunk build --release` (or prints embedding guidance for the library-only `html`/`ai-schema` targets), with an optional `--bundle` flag that runs `cargo packager` for installers. `generate.rs` is an offline, rule-based `--prompt` UI scaffolder (keyword-matches against known patterns and emits a `view!` snippet) — not a live LLM call. `benchmark`/`optimize` wrap `cargo bench`/release-size reporting. See [docs/quickstart.md](docs/quickstart.md).

- **`examples/`**: excluded from the workspace (`Cargo.toml` `exclude = ["examples"]`) since they need their own dependency resolution (wasm-bindgen versions, `cdylib` crate-type for trunk). All are committed and built in CI: `counter-dom` (trunk), `counter-canvas`/`counter-tui` (native `cargo run`), `counter-webview` (native host + nested `ui/` DOM app, skipped by CI's examples job — needs system webview libs), `ssr-page` (HTML string demo), `ai-agent-demo` (headless AI-agent API demo).

## Working in this repo

- Backends must stay independent consumers of `appfront-core` — don't add backend-specific fields to `UITree`/`NodeKind`/`NodeMeta`; extend the AST generically and let each backend interpret it.
- `appfront-dom` and the wasm side of `appfront-canvas` only compile under `wasm32-unknown-unknown`; if you touch either, build with `--target wasm32-unknown-unknown` too, not just native.
- Treat `todo.md` as the source of truth for what phase/feature is actually implemented vs. planned — `spec.txt` describes the eventual full design (including a couple of features, like GPU-compute layout, that are explicitly stretch/future work).
