# Linux webview backend: design

Status: proposed (Phase 24e follow-up). Replaces the `tao` + `wry` Linux path in `tpt-appfront-webview` so that the Linux build no longer compiles `gtk-sys`, `system-deps`, `cfg-expr` or `target-lexicon`.

## Problem

On Linux, `tao` and `wry` build `gtk-rs` against GTK3 and WebKitGTK through `system-deps` at build time. That pulls in `cfg-expr`, which depends on `target-lexicon` (`Apache-2.0 WITH LLVM-exception`, banned under Phase 24e). `target-lexicon` is a build-script dependency, so it never ships in the binary, but the license policy applies to the whole graph.

The rest of the Linux graph (`arboard`, `rfd`, `keyring`, `notify-rust`, `global-hotkey`) uses D-Bus or X11 and has no GTK. It is out of scope and stays as it is.

## Approach

Load GTK3 and WebKitGTK at runtime instead of linking them at build time:

- `libgtk-3.so.0` and `libwebkit2gtk-4.1.so.0` are opened with `libloading` (ISC, already in the tree) on first use. There are no `-dev` packages and no `pkg-config` at build time.
- If either library is missing, `run` returns a clear error that names the package to install, for example `libwebkit2gtk-4.1-0`.
- The loader is a plain function table (`struct Gtk { gtk_init_check: unsafe extern "C" fn(...) ..., ... }`) filled in by symbol lookup. Missing symbols are reported by name at startup.

`tpt-shell`'s Linux backend (`crates/tpt-shell-platform-linux`, MIT/Apache-2.0) uses the same dlopen approach. Its GTK3 module can serve as a reference. Any code we copy keeps its license notice and is recorded in `THIRD_PARTY_NOTICES`. We do not take a dependency on `tpt-shell` itself.

## Module layout

```
crates/tpt-appfront-webview/src/
  manager.rs            # unchanged on Windows/macOS (tao + wry)
  linux/
    mod.rs              # Linux entry point: same signatures as manager's run path
    loader.rs           # libloading wrapper, symbol table, clear missing-lib errors
    sys.rs              # extern "C" fn types for the handful of GTK/GLib/WebKit calls used
    window.rs           # gtk_window: title, size, position, visibility, close-request
    webview.rs          # WebKitWebView: load_uri, evaluate JavaScript, init script
    scheme.rs           # app:// custom URI scheme -> serve() from lib.rs
    ipc.rs              # script message handler -> ipc callback, rate limit and ACL reused
    main_loop.rs        # g_main_context_iteration driver; ControlFlow equivalent
```

A small internal trait sits between `manager.rs` and the two backends, so that the window, scheme, and IPC logic is written once. `serve()`, `Acl`, `IpcRejection` and the rate limiter in `lib.rs` are shared unchanged.

Cargo gating:

```toml
[target.'cfg(not(target_os = "linux"))'.dependencies]
tao = "0.37"
wry = "0.57"

[target.'cfg(target_os = "linux")'.dependencies]
libloading = "0.8"   # already a workspace dependency elsewhere
```

## Symbols

Lookup is by name, and every symbol is checked against the WebKitGTK 4.1 and GTK 3 headers during stage 2 before use. The initial set is:

- GTK 3: `gtk_init_check`, `gtk_window_new`, `gtk_window_set_title`, `gtk_window_set_default_size`, `gtk_window_move`, `gtk_container_add`, `gtk_widget_show_all`, `gtk_widget_destroy`
- GLib / GObject: `g_signal_connect_data`, `g_main_context_iteration`, `g_object_unref`, `g_free`, `g_bytes_new`, `g_memory_input_stream_new_from_data`
- WebKitGTK: `webkit_web_view_new_with_user_content_manager`, `webkit_web_view_load_uri`, `webkit_web_view_evaluate_javascript` (fall back to `webkit_web_view_run_javascript` on older 2.x), `webkit_user_content_manager_add_script`, `webkit_user_content_manager_register_script_message_handler`, `webkit_user_script_new`, `webkit_web_context_register_uri_scheme`, `webkit_uri_scheme_request_finish`

Symbols that are not in this list are not loaded.

## Behavior parity

| Feature | Windows/macOS | Linux (new) |
|---|---|---|
| Window title, size, position | tao | GTK window |
| `app://` scheme serving `dist/` | wry custom protocol | WebKit URI scheme handler, calls `serve()` |
| Initialization script | wry | `WebKitUserScript` at document start |
| IPC (`window.ipc` style) | wry ipc handler | script message handler, same ACL and rate limit |
| `evaluate_script` | wry | `webkit_web_view_evaluate_javascript` |
| Close request | tao `CloseRequested` | GTK `delete-event` |
| Drag and drop | wry | **not in stage 1**; returns `Unsupported` on Linux |
| Devtools | wry | **not in stage 1** |

Behavior that differs is listed in `docs/troubleshooting.md` under the Linux webview.

## Acceptance

1. `cargo tree -p tpt-appfront-webview --target x86_64-unknown-linux-gnu` contains no `gtk`, `gtk-sys`, `tao`, `wry`, `system-deps`, `cfg-expr` or `target-lexicon`.
2. `cargo deny --all-features check bans` passes for the webview crate.
3. CI runs a new `linux-webview` job on `ubuntu-latest`:
   - installs `libgtk-3-0`, `libwebkit2gtk-4.1-0` and `xvfb`;
   - runs `xvfb-run cargo test -p tpt-appfront-webview`;
   - runs the `counter-webview` example under `xvfb-run` for a few seconds and checks that it exits cleanly on a timer.
4. Windows and macOS builds and tests are unchanged.

## Stages

1. Cargo gating and internal trait extraction. Windows and macOS behavior is unchanged, and the existing tests pass.
2. `loader.rs` and `sys.rs`, with unit tests for missing-library and missing-symbol errors. No display needed.
3. Window, `load_uri`, `evaluate_script`, and the main loop.
4. `app://` scheme and IPC with ACL and rate limit.
5. CI `linux-webview` job.
6. Remove the Linux `tao`/`wry` path, then check the bans and the Linux tree.

## Risks

- **No Linux machine locally.** Development is on Windows, so stages 3 onward are validated only in CI under `xvfb`. Early CI runs will probably fail for environment reasons before they fail for code reasons.
- **WebKitGTK API drift.** `webkit_web_view_run_javascript` is deprecated in newer releases, so the loader has to pick the best available symbol at runtime.
- **Feature gap.** Drag and drop and devtools are missing on Linux until a later stage. Apps that need them should check the capability at runtime.
- **Wayland.** GTK3 runs on Wayland through XWayland. Native Wayland is not a goal.
- **Code provenance.** Copying from `tpt-shell` must keep its notices and be recorded in `THIRD_PARTY_NOTICES`.
