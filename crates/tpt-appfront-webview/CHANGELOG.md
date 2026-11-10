# Changelog

All notable changes to `tpt-appfront-webview` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `AppBuilder` for multi-window desktop apps, with `with_window`, `with_acl`,
  `with_max_commands_per_second`, `with_sidecar`, `with_shortcut`, `with_deeplink`,
  `with_single_instance`, `with_crash_reporter`, `with_log_sink`,
  `with_persisted_state`, `with_ipc_rejection` and `with_builtin_capabilities`.
  `run` takes a command handler that returns `Result<(), String>`;
  `run_with_value_handler` also sends each `Ok` value back to the page through
  `window.__appfrontResolve`.
- Capability-based ACL: `Acl`, `Capability` and `ParamSpec`/`ParamKind`. An action
  is dispatched only when a capability grants it and its parameters match the
  declared types.
- `WebviewOptions::on_ipc_rejection` hook, called for oversized, malformed,
  denied, rate-limited and failed IPC messages.
- Built-in capabilities: dialogs (`open_dialog`, `save_dialog`), notifications
  (`notify`), clipboard (`Clipboard`), OS keyring secrets (`get_secret`,
  `set_secret`, `delete_secret`) and camera/microphone requests gated by the ACL
  (`media_capability`, `MediaKind`).
- Sidecar process supervision (`SidecarConfig`, `SidecarSupervisor`), with restart
  after a crash when `restart_on_crash` is set.
- Global shortcuts (`ShortcutRegistry`, `parse_shortcut`), deep links
  (`register_scheme`, `DeepLinkDispatcher`), drag-and-drop events
  (`DragDropDispatcher`), a single-instance guard (`ensure_single_instance`), window
  geometry persistence (`WindowGeometry`) and a panic hook (`install_panic_hook`).
- `tray` feature with a `TrayController` API. The Windows backend is not implemented
  yet: `TrayController::new` returns an error.
- IPC message size cap (`MAX_IPC_MESSAGE_BYTES`, 16 KiB), checked before parsing.
- CLI `--target webview` and `dev --desktop-webview` wiring; `examples/counter-webview`.

### Changed
- **Breaking:** `WebviewOptions::allowed_actions: Vec<String>` is replaced by
  `WebviewOptions::acl: Acl`. Use `WebviewOptions::with_allowed_actions` for the
  old flat list of action names.
- The IPC rate limiter is a small in-crate token bucket instead of `governor`, whose
  `nonzero_ext` dependency the license policy bans. Behavior is unchanged.
- The clipboard is native on Windows only. It uses the Win32 clipboard directly
  (`CF_UNICODETEXT`, via `windows-sys`). On macOS and Linux, `clipboard.read` and
  `clipboard.write` return an error. This is a regression on macOS, which had `arboard`
  before. Pages can use `navigator.clipboard` instead, which WebView2 and WKWebView support.
- Global shortcuts are registered on Windows with `RegisterHotKey`, against a hidden
  message-only window. Presses reach the app as `shortcut` actions with `{"id": ...}`, as
  before. A combination already owned by another app fails at startup and is logged, and
  the app runs without it. On macOS and Linux, shortcut registration fails with a clear
  error, and the app runs without shortcuts. The `global-hotkey` crate is not used, because
  it pulled in `x11rb` and the banned `gethostname`.
- `parse_shortcut` returns a plain `Shortcut` (modifier flags and a key name), not a
  `global-hotkey` type.
- `arboard` and `global-hotkey` are no longer dependencies, so `gethostname` is out of the
  dependency graph on every target.
- `Cargo.toml` inherits `version`, `edition`, `license` and `repository` from the workspace.

### Fixed
- The crate compiles again. It is ported to the wry 0.57 API (`WebViewBuilder`,
  `Request<String>` IPC handlers), and `tao` 0.37 is a regular dependency because wry
  needs a tao window and event loop. The public API is unchanged by the port.
- The 16 KiB IPC size cap now runs on the production dispatch path. Previously it
  was only checked in a test-only helper.
- Sidecar supervisor treats a non-zero exit, a signal or a failed launch as a crash
  (reported through `SidecarConfig::crash_reporter`). Shutdown stops a running child.

## [0.1.0]

### Added
- Initial release: `wry` + `tao` desktop shell hosting `tpt-appfront-dom`'s `trunk`
  build over an `app://` protocol, allowlisted IPC bridge (`on_command`), and
  `window.__appfront.post(action, params)` for custom JS.
