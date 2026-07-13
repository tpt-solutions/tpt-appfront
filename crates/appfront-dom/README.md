# appfront-dom

WASM DOM backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders an `appfront-core::UITree<Msg>` directly to the real DOM via `web-sys` — no virtual DOM, no diffing pass. `mount()` walks the tree once and builds real DOM nodes; `reactive_text()` ties DOM text nodes straight to a `Signal<String>`, batching updates into one `requestAnimationFrame` flush per frame. Keyed `List`/`DataGrid` children are diffed in place via `update_list`, with `VirtualScroll` windowing support. `hydrate_node` implements islands-style partial hydration: only subtrees with listeners/actions or flagged `is_dynamic` get event listeners attached, leaving inert static content untouched.

This crate is `wasm32-unknown-unknown`-only (gated behind `#![cfg(target_arch = "wasm32")]`); it compiles to an empty crate on native targets so a workspace containing it still builds everywhere.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture, and `examples/counter-dom` for a runnable `trunk`-built example.
