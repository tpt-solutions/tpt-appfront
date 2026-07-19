# tpt-appfront-webview

Thin OS-webview desktop shell (wry + tao) for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront), hosting the DOM WASM bundle with no Electron/Node.

Serves a `trunk`-built `dist/` directory — the same `tpt-appfront-dom` WASM app used on the web — over an `app://` custom protocol inside a native window, with an allowlist- and rate-limit-gated native↔JS IPC bridge (`WebviewOptions::allowed_actions`, `max_commands_per_second`).

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-webview = { git = "https://github.com/tpt-solutions/tpt-appfront" }
```

Not published to crates.io yet (`publish = false`) — needs system webview libraries (e.g. `webkit2gtk` on Linux). See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and the [`counter-webview` example](https://github.com/tpt-solutions/tpt-appfront/tree/main/examples/counter-webview).

## License

Apache-2.0
