# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
