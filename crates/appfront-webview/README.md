# appfront-webview

Native webview shell (wry + tao) hosting the [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) DOM backend.

A thin native window, with no bundled Chromium and no npm toolchain, that serves a `trunk build`ed `appfront-dom` app over an `app://` custom protocol using the OS's own webview (WebView2 / WKWebView / WebKitGTK). `WebviewOptions::allowed_actions` allowlists which IPC actions the hosted page may dispatch back to native (closing the Electron-style "open bridge" vulnerability), and `max_commands_per_second` rate-limits the IPC bridge.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture, and `examples/counter-webview` for a runnable native host + nested DOM app (requires system webview libraries — `libwebkit2gtk` + `libsoup3` on Linux).
