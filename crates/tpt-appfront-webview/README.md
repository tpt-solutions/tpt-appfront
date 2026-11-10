# tpt-appfront-webview

The desktop shell for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

This crate opens a native window backed by the OS webview (WebView2 on Windows,
WKWebView on macOS, WebKitGTK on Linux). It serves the `trunk build` output of a
`tpt-appfront-dom` app over an `app://` custom protocol, so there is no bundled
Chromium and no npm toolchain. Actions that the page dispatches (`data-ai-action`
elements, or `window.__appfront.post(action, params)`) are checked against an
ACL and then passed to your Rust code.

The crate is `publish = false` in this workspace.

## Layout

A webview app has two parts:

1. A native host binary (this crate) that opens the window and serves a `dist/`
   directory.
2. A `tpt-appfront-dom` UI crate, built with `trunk`, whose output is that `dist/`.

## API

Entry points:

- `run(opts: WebviewOptions, on_command: F) -> anyhow::Result<()>`, where
  `F: Fn(&str, serde_json::Value) -> Result<(), String> + 'static`. Opens one window
  and runs the event loop until it closes.
- `AppBuilder::new(app_id)` for multi-window apps, sidecars, shortcuts and the other
  features below. `builder.with_window(...).with_acl(...).run(on_command)`, or
  `run_with_value_handler` to return a value to the page.

`WebviewOptions` fields: `title`, `width`, `height`, `dist_dir: PathBuf`, `acl: Acl`,
`max_commands_per_second: u32` and `on_ipc_rejection: Option<Arc<dyn Fn(IpcRejection)>>`.
`WebviewOptions::with_allowed_actions(title, width, height, dist_dir, actions, max_cps)`
builds the options from a flat list of action names with no parameter checks.

Access control:

- `Acl { capabilities: Vec<Capability> }`, with `Acl::validate(action, params)`.
- `Capability { action, params: Vec<ParamSpec> }`, where `ParamSpec` has `name`,
  `required`, `kind: ParamKind` (`String`, `Number`, `Boolean`, `Object`, `Array`,
  `Any`) and an optional default.

Other modules, all re-exported from the crate root:

- Sidecars: `SidecarConfig`, `SidecarSupervisor`, `Stream`, `LogSink`.
- Dialogs and notifications: `open_dialog`, `save_dialog`, `notify`.
- Clipboard: `Clipboard`.
- Secrets (OS keyring): `get_secret`, `set_secret`, `delete_secret`, `SecretError`.
- Shortcuts and deep links: `parse_shortcut`, `ShortcutRegistry`, `register_scheme`,
  `DeepLinkDispatcher`.
- Drag and drop: `DragDropDispatcher`, `DragDropEvent`.
- Media: `media_capability`, `MediaKind`.
- Window and process: `WindowGeometry`, `ensure_single_instance`, `install_panic_hook`,
  `CrashReporter`.
- Tray (feature `tray`): `TrayController`, `TrayEvent`, `TrayMenuItem`. The Windows
  backend is not implemented yet.

## Security

- Only actions granted by the ACL are dispatched. Parameters must match their
  declared types.
- Messages larger than 16 KiB (`MAX_IPC_MESSAGE_BYTES`) are rejected before parsing.
- `WebviewOptions::max_commands_per_second` rate-limits IPC. `AppBuilder` defaults
  to 20 per second.
- `on_ipc_rejection` reports every rejected or failed message, so you can send it to
  telemetry.
- The `app://` handler rejects `..` path segments and paths that resolve outside
  `dist_dir`.

## Example

```rust
use tpt_appfront_webview::{run, WebviewOptions};

fn main() -> anyhow::Result<()> {
    let opts = WebviewOptions::with_allowed_actions(
        "Counter".into(),
        800,
        600,
        "ui/dist".into(),
        vec!["increment".into()],
        20,
    );
    run(opts, |action, params| {
        println!("{action}: {params}");
        Ok(())
    })
}
```

Scaffold a project with `tpt-appfront init --target webview`, or run one with
`tpt-appfront dev --desktop-webview`. See `examples/counter-webview`.

## Features

- `tray`: enables the `tray` module (adds `windows-sys` on Windows).

## License

MIT OR Apache-2.0
