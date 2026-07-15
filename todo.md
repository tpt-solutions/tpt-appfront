# TPT AppFront — Tauri-Parity / Release Roadmap

Priority pillars: desktop (webview shell, for Titan) and web/PWA — both to be made genuinely strong. Mobile (Android/iOS) is a permanent non-goal, not a deferred phase.

## Phase 0 — Housekeeping
- [ ] Add `CHANGELOG.md`; bump crate versions off `0.1.0` in lockstep with the first tagged release (`[workspace.package] version` in root `Cargo.toml`)

## Phase 1 — Titan-blocking desktop shell work (`appfront-webview`)
- [ ] Sidecar process management: spawn/monitor/restart the Go backend, pipe stdout/stderr into a log sink (new `appfront-webview/src/sidecar.rs`, supervisor thread with crash-restart/backoff)
- [ ] IPC/ACL permission model: extend the existing flat `allowed_actions` allowlist (`lib.rs:44-65`) into per-capability/per-window scoped permissions with argument validation, not just action-name allowlisting
- [ ] Secret storage: wrap the `keyring` crate (Windows Credential Manager / macOS Keychain / Linux Secret Service) behind an IPC action so JS never needs secrets bundled client-side
- [ ] Native dialogs (open/save) — via `rfd` or similar, wired through existing IPC dispatch
- [ ] Native notifications — via `notify-rust` or similar
- [ ] System tray / menu bar — via `tray-icon` or similar
- [ ] Clipboard API
- [ ] Drag-and-drop file handling
- [ ] Global shortcuts
- [ ] Deep-link / custom OS URL-scheme handling (runtime registration; see Phase 2 for install-time registration)
- [ ] Multi-window support: move from the single `WindowBuilder`/`WebViewBuilder` call in `run()` to a window registry keyed by ID
- [ ] Persisted window state (position/size) across relaunches
- [ ] WebRTC camera/mic access: wire wry's permission-request hooks, gated through the new ACL model above (not blanket-granted)
- [ ] Verify (not build): confirm CSP/protocol headers on the `app://` custom protocol don't block WebRTC or worker-based JS libs (charts, spreadsheet, rich text, video conferencing UI already embeddable in principle)
- [ ] Single-instance enforcement: named-mutex/lock-file check in `run()` before spawning a second window+sidecar pair
- [ ] Sidecar lifecycle correctness: graceful shutdown of the Go backend on app close/crash, stable port/socket handoff across relaunches
- [ ] Unified logging: sidecar stdout/stderr + Rust shell logs into one sink
- [ ] Auto-launch-at-login (registry run key / macOS LaunchAgent / XDG autostart), capability-gated like the rest of Phase 1
- [ ] Crash reporting hook: panic hook (Rust side) + sidecar crash events surfaced to telemetry

## Phase 1b — PWA/web hardening (`appfront-server/src/pwa.rs`)
- [ ] Web Push notifications: push-subscription endpoint + `push` event handling in the generated service worker
- [ ] Background sync: register a `sync` event so queued offline actions flush on reconnect
- [ ] Update-available UX: `postMessage`/`controllerchange`-based "new version ready, reload to update" flow (today a new service worker installs silently with no signal to the page)

## Phase 2 — Packaging, signing, updates
- [ ] Extend `packager.toml` templating (`appfront-cli/src/templates.rs`) to cover `nsis` (Windows), `pkg` (macOS), `rpm`/`AppImage` (Linux) — currently only `msi`/`dmg`/`appimage`/`deb`
- [ ] Code signing: Authenticode cert config (Windows), Apple notarization/entitlements (macOS) — `cargo-packager` config + CI secrets
- [ ] Install-time deep-link/protocol registration in the installer itself (fold into `packager.toml` templating)
- [ ] Auto-updater client: update-check/apply logic with signature verification against `cargo-packager`'s `generate-updates` manifest (not built yet — today only the artifact generation flag is passed through)
- [ ] CI release pipeline: new `.github/workflows/release.yml`, cross-platform matrix (win/mac/linux) triggered on tag, running `--bundle` + signing + artifact publishing, mirroring the existing `ci.yml` job structure

## Phase 3 — Accessibility & backend hardening
- [ ] Canvas: fill `paint.rs` accesskit gaps for `Container`/`List` (currently no accessible node logic) and `DataGrid`/`paint_data_grid` (currently zero accesskit calls)
- [ ] TUI accessibility: evaluate whether a specific Titan use case needs this; otherwise out of scope given webview is the desktop target

## Phase 4 — "Better than Tauri" differentiators (post-Titan-launch)
- [ ] Production-harden all 5 backends (canvas, HTML, AI-schema, TUI — not just webview/DOM) to the same polish bar
- [ ] Formal plugin API in `appfront-core` (currently nothing exists — no `Plugin`/`plugin_api`/`PluginRegistry`)
- [ ] Cross-backend hot-reload parity: extend the existing `dev_desktop_watch` pattern (`appfront-cli/src/main.rs:315-352`) to `--tui`/`--desktop-webview`; document what `trunk serve` already gives `--web`
- [ ] Binary-size/cold-start benchmarking vs Tauri: documented methodology + real published numbers (`appfront benchmark`/`optimize` commands already exist as the base)
- [ ] AI-native JSON-LD/MCP story: polish/demos on top of the already-built `appfront-ai-schema`/`appfront-mcp` foundations

## Phase 5 — Frontend-framework parity (React/Svelte-level completeness, not just Tauri parity)
Goal: developers get what they'd expect from a "real" frontend framework, without literally cloning React/Svelte APIs.
- [ ] Client-side router: a real hash/history-based router wired to browser location for `appfront-dom` (today only a bare `Signal<String>` route pointer exists in `appfront-core/src/agent.rs:56-92` for AI-agent/devtools purposes — no path-matching, no view-swapping, no browser-location integration). Design as a backend-agnostic route table in `appfront-core` (matches the "extend the AST generically" rule — don't wire browser-only APIs into the core), consumed by `appfront-dom` for real navigation and by `appfront-html`/`appfront-ai-schema` for SSR/crawl-time route resolution
- [ ] Context/DI primitive in `appfront-core`: tree-scoped shared state so deeply nested components don't need state threaded through every constructor arg (today only explicit `Signal` passing exists, no provider/consumer construct)
- [ ] Custom-component tags in `view!`/`rsx!`: extend `appfront-macros/src/view.rs`'s `TAGS`/`ALLOWED` list (currently only `Container/Heading/Text/Button/Input/List/DataGrid`) so a `#[component]`-annotated fn can be used as `<MyComponent prop={x} />` inside the macro, not just via manual `ContainerBuilder::with(...)` composition
- [ ] Async data primitive (`Resource<T>`-style, SolidJS/Svelte-store equivalent): a signal-integrated wrapper for async fetches exposing loading/error/data states, so UI code doesn't hand-roll ad hoc loading flags
- [ ] Enter/exit transition & animation primitives: tie signal-driven visibility changes to CSS transitions in `appfront-dom` and to eased value interpolation in `appfront-canvas` (currently absent — spec.txt only claims "hardware-accelerated animations" as a canvas selling point, nothing implemented)
- [ ] Form validation & multi-field state helpers: beyond the existing `on_input` two-way binding (`view.rs:11-14`), add validation combinators and a way to aggregate multi-field form state/errors
- [ ] Error boundaries: a subtree-isolation construct (`catch_unwind`-based) so one panicking component doesn't crash the whole render tree
- [ ] Portals: a way to render a node outside its logical tree position (modal/tooltip/toast layers) — none of the builder APIs (`ContainerBuilder::with`/`container`) support this today, they only append within the current parent
- [ ] Component-level testing utilities: a render-and-query helper for `appfront-dom`/`appfront-canvas` (Testing-Library equivalent) — today only `appfront-tui`'s `TestBackend` and `appfront-core` signal unit tests exist at that granularity
- [ ] Code-splitting/lazy loading: defer-mounting a subtree or splitting the WASM bundle so large apps don't ship one monolithic `.wasm` (today `appfront-dom::mount`/`hydrate_node` walk and hydrate the whole tree up front; islands-style hydration skips listener attachment on static subtrees but doesn't defer loading)
- [ ] Live DevTools: upgrade `appfront-core/src/devtools.rs` from a static text/HTML report generator (`inspect_tree`/`inspect_state`/`render`/`to_html`) into an interactive live inspector (in-page overlay or browser extension), the way React/Vue DevTools work

## Phase 6 — Genuine differentiators (beyond parity with anything else)
Ideas that fall directly out of AppFront's architecture (one `UITree`, one signal core, five backends, AI-native schema) and aren't replicable by React/Svelte/Tauri without rebuilding from scratch.
- [ ] Cross-backend snapshot/structural testing: one test assertion against the abstract `UITree` verifies correctness on canvas, DOM, HTML, and TUI simultaneously — a test-authoring primitive no single-backend framework can offer
- [ ] `appfront preview --all-backends` dev command: render the same running app in a canvas window, browser tab, and TUI pane at once for instant cross-backend visual QA
- [ ] "JS-optional progressive enhancement" as a shipped, CI-enforced guarantee: since HTML/SSR (`appfront-html`) is a real render target rather than a bolted-on meta-framework, add a CI check that the app is meaningfully usable with JavaScript fully disabled
- [ ] Auto-generated E2E regression tests from the AI schema: walk `appfront-ai-schema`'s output (every actionable node already declares its action + params) and auto-generate tests that exercise every interactive element — near-zero hand-written test code for full-app coverage
- [ ] Compile-time Msg/action consistency check: verify at compile time (likely in `appfront-macros`) that every AI-actionable node's declared action maps to a real `Msg` variant, catching a class of bug JS frameworks can only catch at runtime
- [ ] Structural time-travel debugging in `devtools.rs`: since `Signal`/`UITree` are plain data, record full state history and support rewind/replay natively in the reactive core, richer than middleware-based tools like Redux DevTools
- [ ] Generic undo/redo & change-explanation utility: expose `appfront-core/src/reconcile.rs`'s existing tree-diffing (currently only used internally for DOM updates) as a public API so any app gets "what changed / undo this" almost for free
- [ ] Compile-time dead-class elimination: extend the `class!` macro's existing compile-time unknown-class checking (`appfront-core/src/styling.rs`) into full unused-class tree-shaking — more reliable than JS tooling's string-scanning heuristics (PurgeCSS) since the macro has exact static knowledge of what's used
- [ ] Market/harden the canvas backend as a true zero-runtime-dependency single-binary distribution option (no WebView2/webkitgtk required) — already technically true via `eframe`'s `glow` renderer, but not yet called out or tested as a first-class distribution target

## Phase 7 — Correctness-by-construction (reduce reliance on hand-written tests)
Goal: prove invariants once instead of enumerating example test cases; make invalid states unrepresentable where possible.
- [ ] Property-based tests (`proptest`) for `appfront-core/src/signal.rs`'s diamond-dependency batching: "every effect runs exactly once per `batch()`, regardless of update order" as a generated-case law, not a handful of hand-picked examples
- [ ] Property-based tests for `appfront-core/src/reconcile.rs`'s keyed diffing: "diffing a tree against itself produces zero ops," "output never drops or duplicates a key," "child order is preserved except for explicit moves"
- [ ] Property-based tests for `appfront-core/src/virtual_scroll.rs`'s `visible_range` math: "range never exceeds `total_items`," "window size is monotonic in viewport height" — the off-by-one-prone arithmetic that example tests are worst at catching
- [ ] Make invalid states unrepresentable: `VirtualScroll` config (`item_height`/`viewport_height`) via `NonZeroU32` or a validating constructor returning `Result`, and `DataGrid` column/row-count mismatches rejected at construction, instead of testing for the bad states at runtime
- [ ] Lightweight formal verification (`kani`, AWS's bounded model checker) applied to the arithmetic-heavy spots: `appfront-canvas/src/layout.rs` layout calculations and `virtual_scroll.rs` index math — prove no-panic/no-overflow and functional postconditions on small, self-contained functions
- [ ] Differential/oracle testing across backends: use one backend's render as ground truth and auto-check the others agree structurally, instead of hand-asserting expected output per backend (shares infrastructure with Phase 6's cross-backend testing, applied here as a correctness technique rather than a QA feature)
- [ ] Mutation testing (`cargo mutants`) run against the existing test suite to identify which tests are load-bearing vs. which pass regardless of the code being broken, before writing more tests blindly
