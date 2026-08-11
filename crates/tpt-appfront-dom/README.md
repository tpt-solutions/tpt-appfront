# tpt-appfront-dom

The fine-grained-reactive real-DOM backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

`mount` walks a `UITree` once and creates real DOM nodes directly via `web-sys`,
then keeps a `MountedRoot` record so subsequent renders update attributes and
children *in place* instead of tearing the subtree down and rebuilding it. Event
handlers dispatch an app-defined `Msg` back through a caller-supplied callback.

This crate only does anything on `wasm32` targets; on other targets it compiles
to an empty crate so the workspace still builds natively — point your editor's
rust-analyzer at `wasm32-unknown-unknown` to get type-checking for it
(see [docs/editor-setup.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/editor-setup.md)).

## Features

- **Fine-grained updates** — `reactive_text` ties a DOM text node directly to a
  `Signal<String>`, bypassing the tree entirely; updates are coalesced into one
  `requestAnimationFrame` flush per frame.
- **Keyed list diffing** — `List`/`DataGrid` items are reconciled by stable `key`
  (add/remove/reorder existing DOM nodes) instead of being rebuilt wholesale.
- **Streaming hydration** — `hydrate` resumes a server-rendered page by matching
  each `UITree` node to its DOM element via `data-appfront-id` and attaching
  listeners (no DOM mutation). Islands of interactive subtrees hydrate while
  static content stays inert.
- **`unmount`** — every mount returns a `MountedRoot` whose `unmount()` removes
  listeners and drops effect handles, so route changes / modal closes don't leak.
- **Virtual scrolling** — drives `NodeMeta::virtual_scroll` to render only the
  visible window (+spacers) of long lists.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-dom = "0.1"
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["Document", "Window", "Element"] }
```

## Example

```rust
use tpt_appfront_core::{Signal, UITree};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let document = web_sys::window().unwrap().document().unwrap();
    let body = document.body().unwrap();
    let count = Signal::new(0i32);
    let ui: UITree<Msg> = view! { /* ... */ };
    tpt_appfront_dom::mount(&body, &ui, |msg| { /* dispatch */ })?;
    Ok(())
}
```

Build with `trunk build` (see [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md)).

## License

MIT OR Apache-2.0
