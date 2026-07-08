# TPT AppFront — Build Checklist

Architecture: one `UITree` + signal core, with DOM, Canvas, HTML(SSR), and AI-Schema as pluggable backend crates. See `spec.txt` for the original design doc.

## Phase 0 — Repo & Workspace Setup
- [x] `git init`
- [x] `.gitignore` (Rust/WASM/trunk artifacts)
- [x] README stub with quickstart pointer
- [x] Cargo workspace with empty crates: `appfront-core`, `appfront-dom`, `appfront-canvas`, `appfront-html`, `appfront-ai-schema`, `appfront-macros`, `appfront-cli`
- [ ] GitHub Actions CI: build `wasm32-unknown-unknown` + native, `cargo test`, `cargo clippy`, plus building everything under `examples/`

## Phase 1 — Day 1: Reactive Signal System (`appfront-core`)
- [x] `Signal<T>` with `new/get/set`, `Rc<RefCell<T>>`-based, automatic dependency tracking
- [x] Unit tests: signal updates propagate only to dependents

## Phase 2 — Day 2: The UITree AST (`appfront-core`)
- [x] `UITree` enum: `Container`, `Text`, `Button`, `Input`, `List`, `Heading`, `DataGrid`, etc., each with `class`, `events`, `children`
- [x] `serde::Serialize`/`Deserialize` derive (generic over the app's `Msg` type)
- [x] Builder API matching spec's `UITree::container(|c| {...})` ergonomics

## Phase 3 — Day 3: DOM Backend (`appfront-dom`) — first web milestone
- [x] `web-sys`/`wasm-bindgen` renderer: `UITree` → real DOM nodes, minimal `create_element`/`set_text_content` calls
- [x] Event listener wiring (`onclick`) back to a `Msg` dispatch callback
- [x] Fine-grained updates: `reactive_text` ties a DOM text node directly to a `Signal<String>`, updates bypass the tree entirely
- [x] `examples/counter-dom` builds and serves via `trunk build` / `trunk serve` (verified: valid JS+WASM bundle, correct HTML shell)
- [ ] `trunk`-based dev server wired to `appfront dev --web` CLI command (CLI itself is Phase 8)
- [~] **Milestone check:** build/serve pipeline verified end-to-end (curl'd the served page, confirmed wasm bundle loads); clicking the button in an actual browser window was not manually driven in this session — no browser automation available here. Recommend a quick manual check: `cd examples/counter-dom && trunk serve`, open http://127.0.0.1:8080, click "+1".

## Phase 4 — Day 4: Canvas Backend (`appfront-canvas`)
- [ ] winit + wgpu + egui window, `UITree` → egui widgets (`Div`→`Frame`, `Button`→`Button`, etc.)
- [ ] `cosmic-text` for text shaping
- [ ] Shared layout via `taffy` (not GPU compute shaders) for v1
- [ ] Runs on both desktop (native) and web (WASM canvas) targets

## Phase 5 — Day 5: Compile-Time Optimizer Macro (`appfront-macros`)
- [ ] `#[appfront::component]` proc macro: analyzes a `UITree`-returning fn, detects static vs. dynamic/interactive content, generates minimal creation code
- [ ] Static/dynamic detection (virtual-scroll/memoization codegen may be deferred — mark TODO if so)

## Phase 6 — Day 6: AI Schema Backend (`appfront-ai-schema` + `appfront-html`)
- [ ] Freeze format first: `docs/ai-schema.md` (JSON-LD shape + custom AI Schema shape)
- [ ] `UITree` → JSON-LD (structured data / rich snippets)
- [ ] `UITree` → custom AI Schema (interactive elements, actions, params)
- [ ] `appfront-html`: `UITree` → semantic HTML string (SSR/SSG), including `data-ai-action`/OpenGraph tags

## Phase 7 — Day 7: Smart Router
- [ ] Axum server: detect client type (User-Agent/query param) → human (WASM+DOM/Canvas), crawler (HTML backend), AI agent (AI Schema backend), social bot (OpenGraph)
- [ ] Wire into `appfront-cli`: `appfront dev --web`, `appfront build --target <target>`

## Phase 8 — CLI & Project Scaffold (`appfront-cli`, `clap`)
- [ ] `appfront init <name>` — scaffold matching spec's project structure, runnable with zero manual edits
- [ ] `appfront dev --desktop` (native winit window, hot-reload if feasible)
- [ ] `appfront build --target <target>`
- [ ] `examples/` directory (DOM counter, Canvas counter, minimal SSR page) built in CI
- [ ] `docs/quickstart.md`: install → init → dev → build

## Phase 9 — Streaming Hydration (Resumability)
- [ ] Server renders HTML + serializes state (`serde_json`) into the page
- [ ] Client (WASM) resumes: attaches listeners to existing DOM nodes instead of re-rendering

## Phase 10 — Programmatic AI Agent API
- [ ] `appfront::query_state()`, `appfront::navigate_to()`, `appfront::trigger_event()` headless API

## Phase 11 — Stretch / Optional
- [ ] GPU-accelerated layout via WebGPU compute shaders (feature-flagged; `taffy` remains default)
- [ ] Runtime `AutoOptimizer` (frame-time profiling → auto-toggle texture caching/virtual scrolling)
- [ ] `appfront generate --prompt "..."` AI-assisted UI generation
- [ ] `appfront benchmark` / `appfront optimize --auto` CI pipeline commands
- [ ] Styling macro layer (Tailwind-like `.class("bg-blue-500 p-4")` wrapper)
- [ ] PWA/offline service worker

## Verification checkpoints
- [ ] Phase 3: `trunk serve` runs counter app in browser; devtools confirms only the text node mutates on click
- [ ] Phase 4: same counter `UITree` renders via native `cargo run` and via WASM canvas in-browser
- [ ] Phase 6/7: `curl -A "Googlebot" ...` returns semantic HTML; normal browser request returns WASM app shell
- [ ] `cargo test` + `cargo clippy --all-targets` pass in CI for native and `wasm32-unknown-unknown` at each phase boundary
