# tpt-appfront-core

UITree AST and the reactive Signal system for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront): virtual scroll, styling utilities, devtools inspector, and static-tree caching, shared by every backend.

TPT AppFront lets you write a UI once in Rust as an abstract `UITree<Msg>` and render it to multiple backends (native/WASM canvas, reactive DOM, semantic HTML, AI/JSON-LD schema, terminal UI, or an OS-webview desktop shell) from one codebase. This crate has no opinion on rendering — it's the shared AST and reactive core every backend crate builds on.

```toml
[dependencies]
tpt-appfront-core = "0.1"
```

See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture and [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md) to get started.

## License

Apache-2.0
