# appfront-core

Backend-agnostic UI tree and reactive signal system for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Write a UI once as an abstract `UITree<Msg>` (generic over your own `Msg` enum) using the chainable `ContainerBuilder` API, and render it with any of the AppFront backend crates: `appfront-dom` (WASM DOM), `appfront-canvas` (egui/taffy), `appfront-html` (semantic HTML/SSR), `appfront-ai-schema` (JSON-LD / AI-agent schema), `appfront-tui` (ratatui terminal), or `appfront-webview` (native webview shell).

Also included:
- `signal.rs` — a from-scratch SolidJS-style reactive system (`Signal<T>`, `create_memo`, `batch`).
- `virtual_scroll.rs` — a windowed list rendering primitive.
- `styling.rs` + the `class!` macro — curated, compile-time-checked Tailwind-like utility classes.
- `static_tree.rs` — a cache for provably-static subtrees hoisted by the `view!` macro.
- `reconcile.rs` — a backend-agnostic keyed-diff primitive.
- `devtools.rs` — a plain-text/HTML tree and signal-activity inspector.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture and [spec.txt](https://github.com/tpt-solutions/tpt-appfront/blob/main/spec.txt) for the design doc.
