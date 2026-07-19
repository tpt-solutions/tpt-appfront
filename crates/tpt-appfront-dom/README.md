# tpt-appfront-dom

Fine-grained-reactive real DOM backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) (web-sys), wasm32-only, no virtual DOM.

Mounts a `UITree<Msg>` directly onto real DOM nodes and ties text/list updates straight to `tpt-appfront-core` `Signal`s, batching into one `requestAnimationFrame` flush per frame. Supports keyed list/DataGrid diffing, virtual scroll windowing, and islands-style partial hydration.

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-dom = "0.1"
```

Only compiles under `wasm32-unknown-unknown` (an empty crate on native). See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md).

## License

MIT OR Apache-2.0
