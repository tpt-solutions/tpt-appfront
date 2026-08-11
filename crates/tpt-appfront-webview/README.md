# tpt-appfront-webview

The desktop webview shell for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

A thin wrapper around `wry` + `tao` (the same stack Tauri uses). It hosts the
`trunk build` output of an `tpt-appfront-dom` app inside the OS's own webview
(WebView2 / WKWebView / WebKitGTK) — **no bundled Chromium and no npm/Node
toolchain** — and bridges DOM events back to native Rust over a small, allowlisted
IPC channel. This is the primary "beat Tauri/Electron" target.

## Layout

A webview app has two parts:

1. A native **host** binary (this crate) that opens the window and serves a `dist/`
   directory produced by `trunk build`.
2. A `tpt-appfront-dom` **UI** crate (built with `trunk`) under `dist/`.

## Features

- **`run(WebviewOptions, on_command)`** — opens the window and serves the `dist/`
  over an `app://` custom protocol; handles the IPC bridge.
- **Allowlisted IPC** — `WebviewOptions::allowed_actions` is an explicit allowlist
  of action strings a hosted page may dispatch; anything not granted (or with
  out-of-contract params) is rejected — avoiding the Electron-style open-bridge
  vulnerability. Messages are size-capped and rate-limited per second
  (`WebviewOptions::max_commands_per_second`).
- **No renderer to ship** — you reuse `tpt-appfront-dom`/`tpt-appfront-html`, which
  already render in pure Rust.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-webview = "0.1"
```

## Example

```rust
use tpt_appfront_webview::{run, WebviewOptions};

let opts = WebviewOptions {
    allowed_actions: ["increment".to_string()].into_iter().collect(),
    ..Default::default()
};
run(opts, |cmd| {
    // `cmd.action`/`cmd.params` already validated against `allowed_actions`
    println!("command: {:?}", cmd);
})?;
```

Scaffold one with `tpt-appfront init --target webview` (or `dev --desktop-webview`).
A real window-open/click smoke test needs a display + WebView2/WKWebView runtime,
which CI sandboxes lack — the crate is still verified by `cargo build` + `clippy`.

## License

MIT OR Apache-2.0
