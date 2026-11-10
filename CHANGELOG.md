# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `tpt-appfront` umbrella crate: feature flags for each backend (`dom`, `canvas`, `html`, `ai-schema`, `server`, `tui`, `mcp`, `test`, `templates`) and a `prelude`. The `view!`, `rsx!` and `#[component]` macros resolve core paths through `proc-macro-crate`, so the umbrella alone is enough. The webview crate is not behind a feature because it is not published.
- `tpt-appfront-test` crate: a headless `Harness` (find, click, type, press key, drain and assert emitted messages), `lint_tree` (accessibility and AI-affordance linter) and `assert_accessible`.
- `tpt-appfront-test` `parity` feature: `parity_gaps` reports which words the HTML, terminal and AI-schema backends drop. `assert_parity` fails when HTML or the terminal drops required text.
- `tpt-appfront-templates` starter crate and `tpt-appfront-cli` starters for canvas and TUI both include a test that calls `assert_accessible`.
- `tpt-appfront generate --llm` providers: `gemini`, `groq`, `mistral`, `together`, `glm`, `deepseek`, `qwen`, `lmstudio` and `azure-openai` (`--base-url` is required for Azure).
- `generate --llm` chooses the model from the provider's live model list when `--model` is omitted. `generate --list-models` prints that list.
- `tpt-appfront doctor` checks the rustc version against the workspace `rust-version`, and the wasm test runner against `Cargo.lock`.
- `tpt-appfront completions <shell>` prints a completion script for bash, zsh, fish, powershell or elvish.
- `tpt-appfront-core`: event handlers on `NodeRef`: `on_focus` and `on_blur` (messages), `on_keydown` (returns `Option<Msg>`, where `Some` also stops the browser default), `on_pointer_down` and `on_paste`. New types: `KeyEvent`, `PointerEvent`, `OnKeyDown`, `OnPointerDown`, `OnPaste`.
- `tpt-appfront-dom`: the new handlers are wired on mount and on hydration, and removed on unmount. Browser tests in `tests/events.rs` run in the CI `wasm-tests` job.
- `tpt-appfront-canvas`: focus, blur, keydown and pointer-down handlers on buttons, text inputs, text areas and non-widget nodes. A key taken by `on_keydown` is consumed before the widget sees it. Paste events are dispatched to the focused node's `on_paste`. Core utility classes the canvas does not render are logged once per name.
- `tpt-appfront-tui`: focus, blur and keydown handlers. Tab and the arrow keys move focus and dispatch `on_blur` and `on_focus`. The focused node's `on_keydown` sees each key first.
- `tpt-appfront-webview`: `AppBuilder::run_with_value_handler`, so app IPC commands can resolve the page's `requestId` promise with a JSON value. The existing `run` is unchanged.
- Repository: `CONTRIBUTING.md` (issues only for now), issue templates, `dependabot.yml`, and a `release.yml` that performs a dry run unless the `RELEASE_PUBLISH` repository variable is `true` or the workflow is started with `dry_run` unchecked.

### Changed
- **Breaking:** `tpt-appfront-server::serve` returns `std::io::Result<()>` instead of panicking on a bind or serve error. Callers need `.await?` or similar.
- **Breaking:** `tpt-appfront-canvas::run_web` returns `Result<(), JsValue>` and reports a missing window, document or canvas as an error instead of panicking.
- **Breaking:** `tpt-appfront-tui`'s `TuiDriver::on_key` returns `Vec<Msg>` (all messages from one key, in order) instead of `Option<Msg>`. Callers that used `if let Some(msg)` should iterate instead.
- `generate --model` is optional. When it is omitted, the model comes from the provider's live model list.
- The scaffolded component from `add component` takes an `on_activate` callback, so its generated signature changed.
- `init` scaffolds depend on the `tpt-appfront` umbrella with feature flags, and generated code imports from `tpt_appfront::`.
- `tpt-appfront-dom`: patches remove stale class, `data-ai-*` and author attributes. `Portal` content is hydrated. Mount paths return errors instead of panicking when there is no window or document.
- `tpt-appfront-webview`: `TrayController::new` returns an explicit error, because the native tray is not implemented yet.
- **Breaking:** `tpt-appfront-templates::LoginFormConfig` gained `password` and `on_input` fields. `on_submit` is now a plain `Msg`. The app keeps the field values in its own state (see the templates changelog).

### Fixed
- `tpt-appfront-webview`: the production IPC path enforces the 16 KiB message cap before parsing.
- `tpt-appfront-webview`: the sidecar supervisor restarts a backend that exits with a non-zero status or a signal, and does not restart one that exits cleanly with code 0. Such failures go to `SidecarConfig::crash_reporter`. Shutdown stops a running child.
- `tpt-appfront-canvas`: the `DataGrid` virtual-scroll paint reads the real source row from `GridRowKind::Data(idx)`, instead of inferring it from the taffy child position.
- `tpt-appfront-core`: `WebStorage` in `store.rs` declares `web-sys` only for `wasm32`.
- `tpt-appfront-templates`: `login_form` reads the password. Its inputs report edits through `on_input`, and the "Sign in" message no longer carries an empty password.
- `tpt-appfront-macros`: custom component tags such as `<LoginForm/>` resolve `tpt-appfront-templates` through the calling crate's dependencies or the umbrella, instead of requiring a direct dependency.
- `tpt-appfront-webview`: the crate compiles again. It is ported to the wry 0.57 API, and `tao` 0.37 is a regular dependency.
- `tpt-appfront-cli`: `doctor` probes `cargo packager --version` instead of only checking `cargo` on `PATH`. A missing install is a warning.

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
