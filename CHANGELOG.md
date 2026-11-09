# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `generate --llm` chooses the model from the provider's live model list when `--model` is omitted, and `generate --list-models` prints that list. Adds GLM, DeepSeek and Qwen. No model ids are pinned any more.
- `tpt_appfront_test::assert_accessible` panics with every accessibility issue. The canvas and TUI starters include a test that calls it.
- `tpt_appfront_test::parity` (feature `parity`): reports which words HTML, the terminal and the AI schema drop, and `assert_parity` fails when HTML or the terminal drop required text.
- `tpt-appfront doctor` checks rustc against the workspace's `rust-version`, and the wasm test runner against `Cargo.lock`.
- `tpt-appfront` (new umbrella crate): feature flags for each backend and a `prelude`. `view!`, `rsx!` and `#[component]` resolve their core paths through `proc-macro-crate`, so depending on the umbrella alone works.
- `tpt-appfront-test` (new crate): a headless `Harness` (find, click, type, press key, assert emitted messages) and `lint_tree`, an accessibility and AI-affordance linter.
- `tpt-appfront-cli`: `generate --llm` providers `gemini`, `groq`, `mistral`, `deepseek`, `together`, `lmstudio` and `azure-openai` (`--base-url` required).
- `tpt-appfront-canvas`: paste events are dispatched to the focused node's `on_paste`. Core utility classes the canvas does not render are logged once per name.
- Repository: `CONTRIBUTING.md` (issues only), issue templates, `dependabot.yml`, and a `release.yml` that dry-runs unless `RELEASE_PUBLISH` is set.

### Changed
- **Breaking:** `tpt-appfront-server::serve` returns `std::io::Result<()>` instead of panicking on a bind or serve error. Callers now need `.await?` or similar.
- **Breaking:** `tpt-appfront-canvas::run_web` reports a missing window, document or canvas as an error rather than panicking (it already returned `Result`).
- `tpt-appfront-cli`: `generate --model` is optional. Previously it defaulted to `claude-sonnet-4-5`, which overrode every provider's own default. The Anthropic default is now `claude-sonnet-5-5`.
- The scaffolded component takes an `on_activate` callback, so `component_rs` output changed signature.
- `tpt-appfront-dom`: patches remove stale class, `data-ai-*` and author attributes; `Portal` content is hydrated; mount paths return errors instead of panicking when there is no window or document.

### Fixed
- `tpt-appfront-webview`: the production IPC path now enforces the 16 KiB message cap before parsing. The check previously lived only in a function that production did not call.
- `tpt-appfront-webview`: the sidecar supervisor restarts a backend that exits with a non-zero status or a signal (it previously treated every exit as clean). Such exits go to the new `SidecarConfig::crash_reporter`. Shutdown stops a running child promptly.
- `tpt-appfront-webview`: `TrayController::new` returns an explicit error, because the Windows backend does not yet show an icon.
- `tpt-appfront-cli`: `completions <shell>` prints a completion script for bash, zsh, fish, powershell or elvish.
- `tpt-appfront-cli`: `init` scaffolds depend on the `tpt-appfront` umbrella with feature flags. Generated code imports from `tpt_appfront::…`.
- `tpt-appfront-macros`: `view!`, `rsx!` and `#[component]` resolve core paths through the calling crate, so the umbrella alone is enough.

### Added
- `tpt-appfront-tui`: focus, blur and keydown handlers now run. Tab, Shift-Tab and the arrow keys move focus
  and dispatch `on_blur`/`on_focus`; the focused node's `on_keydown` sees each key first.
  **Breaking:** `TuiDriver::on_key` now returns `Vec<Msg>` (all messages from one key, in order) instead of
  `Option<Msg>`. Callers that used `if let Some(msg)` should iterate instead.
- `tpt-appfront-canvas`: focus, blur, keydown and pointer-down handlers on buttons, text inputs, text areas and
  non-widget nodes. A key taken by `on_keydown` is consumed before the widget sees it. Checkboxes and selects
  do not have these handlers yet.
- `tpt-appfront-core`: web event handlers on `NodeRef`: `on_focus` and `on_blur` (messages),
  `on_keydown` (returns `Option<Msg>`; `Some` also stops the browser default), `on_pointer_down`
  (client position, button and modifiers) and `on_paste` (clipboard text). Types: `KeyEvent`,
  `PointerEvent`, `OnKeyDown`, `OnPointerDown`, `OnPaste`.
- `tpt-appfront-dom`: the new handlers are wired on mount and on hydration, and removed on unmount.
  Browser tests in `tests/events.rs` (run by the CI `wasm-tests` job).
- `tpt-appfront-webview`: `AppBuilder::run_with_value_handler` — app IPC
  commands can now resolve the page's `requestId` promise with a JSON
  payload (the legacy `run` keeps its fire-and-forget contract; nothing is
  delivered to pages that didn't ask).
- `tpt-appfront-templates`: backend-agnostic starter UIs (`login_form`,
  `dashboard_shell`, `settings_list`) as stateless `UITree` builder functions.
- `tpt-appfront-cli`: `ingest` (HTML → `view!` skeleton), `add component` /
  `add page` scaffolding, `doctor` pre-flight check, `init` `tpt-appfront-tui`
  target, and published-install vs path-dep resolution via `dep_ref()`.
- `tpt-appfront-canvas`: virtual-scroll windowing for `List`/`DataGrid`,
  utility-class (`class!`) styling adapter, AccessKit feature, raw-`egui`
  escape-hatch example (`examples/node-graph`).
- `tpt-appfront-dom`: `Textarea`/`Checkbox`/`Select`/`Radio` node kinds with
  `on_input`/`on_toggle` wiring, `MountedRoot::unmount` closure/effect cleanup.
- `view!` macro: `List`/`DataGrid` tags, two-way `on_input` binding,
  `{if}`/`{for}` control flow, and component-tag composition.
- `tpt-appfront-server`: per-peer-IP rate limiting on `POST /command`
  (migrated off the global bucket), configurable `CorsPolicy`, ETag /
  `If-None-Match` caching wired through (`cached_html`/`cached_json`/
  `opengraph_cache`), `serve()` over `into_make_service_with_connect_info`.

### Fixed
- `tpt-appfront-webview` now compiles (was missing 10 dependencies used by
  clipboard/dialog/notify/secret/shortcut/single-instance/tray/deeplink/
  window-state code; plus a duplicate `MAX_IPC_MESSAGE_BYTES` const and a stale
  `handle_ipc` signature).
- `tpt-appfront-server` dead ETag caching path; `crawler_html`/
  `social_opengraph`/`ai_agent_json` handler signatures corrected to take
  `headers` and return `Response`.
- Pre-rename `appfront_*` crate references fixed to `tpt_appfront_*` across
  `tpt-appfront-html`/`dom`/`macros`/`canvas`, core tests, and
  `examples/counter-webview`.
- `tpt-appfront-core` `WebStorage` (`store.rs`) now declares `web-sys` under
  `cfg(target_arch = "wasm32")`.
- `tpt-appfront-dom` missing imports (`Router`/`create_effect`/`reconcile_keys`)
  and the previously-undefined `drop_closures_for` function.
- `tpt-appfront-canvas` `DataGrid` virtual-scroll paint bug: `paint.rs` now
  reads the real source row from `GridRowKind::Data(idx)` instead of inferring
  it from taffy child position (which broke once spacer rows shifted).
- `tpt-appfront-server` e2e `ServerGuard` hang on Windows (listener needed
  `set_nonblocking(true)` before `from_std()`).

### Notes
- All crates start at `0.1.0` for the first tagged release. Versions are
  managed centrally in the root `Cargo.toml` via `[workspace.package]`.
- After the first tag is cut, bump `[workspace.package] version` here and
  `cargo update -p <crate> --precise <ver>` for any published crates, then
  add a dated section below.
- **Security posture (all deployment modes):** `tpt-appfront-server`'s CSRF
  opt-in (`router/csrf.rs::verify`) allows requests through when no CSRF cookie
  is present — intended as opt-in per-client protection, not enabled by default.
  The `POST /command` rate limiter is in-memory and per-process (per-peer-IP via
  `PeerIpKeyExtractor`); it does not survive restarts or span multiple server
  instances, so front it with shared-rate-limiting/load-balancing infra in
  multi-instance production.

## [0.1.0] - First tagged release

### Added
- Reactive `Signal<T>` core with dependency tracking, `create_memo`, and
  `batch()`-based diamond-dependency dedup (`tpt-appfront-core`).
- `UITree` AST (Container/Heading/Text/Button/Input/List/DataGrid/Textarea/
  Checkbox/Select/Radio/Portal) with builder API, serde, and virtual-scroll
  primitive.
- DOM backend (`tpt-appfront-dom`): keyed list diffing, fine-grained
  `reactive_text`, rAF-coalesced updates, islands hydration, `unmount`.
- Canvas backend (`tpt-appfront-canvas`): egui/glow renderer, taffy layout,
  virtual scrolling, utility-class styling, optional accesskit, optional
  full-text shaping, AutoOptimizer.
- HTML/SSR backend (`tpt-appfront-html`) and AI-schema backend
  (`tpt-appfront-ai-schema`) with JSON-LD + custom AI Schema output.
- `tpt-appfront-macros`: `#[appfront::component]` and `view!`/`rsx!` templating
  macro with static-subtree hoisting, `List`/`DataGrid`, two-way binding,
  `{if}`/`{for}`, and `class!` compile-time checks.
- `tpt-appfront-server`: SmartRouter with ClientKind detection, PWA/offline SW,
  `POST /command`, per-peer rate limiting, security headers, ETag caching.
- `tpt-appfront-mcp`: JSON-RPC over stdio MCP server exposing the agent API.
- `tpt-appfront-tui`: ratatui terminal backend with keyboard-driven dispatch.
- `tpt-appfront-webview`: wry/tao desktop shell with IPC allowlist + rate
  limiting and a 16 KiB IPC message-size cap.
- `tpt-appfront-cli`: `init`/`dev`/`build`/`generate`/`ingest`/`add`/`doctor`/
  `benchmark`/`optimize`/`--bundle`.
- `tpt-appfront-templates`: `login_form`/`dashboard_shell`/`settings_list`.
