# tpt-appfront-dom

The real-DOM backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

`mount` walks a `UITree` once and creates real DOM nodes with `web-sys`. It keeps a
`MountedRoot` so that later renders update attributes and children in place instead
of rebuilding the subtree. Event handlers send an app-defined `Msg` to a callback
you provide. There is no virtual DOM.

This crate only compiles for `wasm32-unknown-unknown`. On other targets it is empty,
so the workspace still builds natively. To get type-checking in your editor, point
rust-analyzer at the wasm32 target.

## Public API

Mounting and updating (`container` is a `web_sys::Element`, `dispatch` is
`Rc<dyn Fn(Msg)>`):

- `mount(container, &ui, dispatch) -> Result<MountedRoot<Msg>, JsValue>`. Returns a
  handle with `render(&mut self, &UITree<Msg>)` and `unmount(self)`.
- `render(container, view: Rc<dyn Fn() -> UITree<Msg>>, dispatch) -> Result<EffectHandle, JsValue>`.
  Re-renders when the signals read by `view` change. Keep the returned handle alive.
- `render_with(...)` takes the same arguments plus an `auto_optimize` flag.
- `mount_router(container, &Router<Msg>, dispatch)` and `mount_router_with(...)`.
  Route changes reconcile the container. `navigate_path` moves the router to a path.
- `update_list(document, ul, old_items, new_items, &dispatch)`: keyed reconciliation of
  `List` and `DataGrid` children.
- `reactive_text(&Document, Signal<String>) -> Result<(Node, EffectHandle), JsValue>`.
  Ties a text node to a signal. Updates are coalesced into one `requestAnimationFrame`
  flush per frame.
- `hydrate(container, dispatch)` (requires `Msg: DeserializeOwned`). Reads the state
  payload from `<script id="__APPFRONT_STATE__">` and attaches listeners to
  server-rendered elements matched by `data-appfront-id`. It does not create or move
  DOM nodes.

## Features

- **In-place updates.** `MountedRoot::render` and `render` change only what differs.
- **Keyed lists.** `List` and `DataGrid` children are reconciled by `key`, so items
  are added, removed and reordered without rebuilding the list.
- **Virtual scrolling.** Set `NodeMeta::virtual_scroll` to render only the visible
  window of a long list, plus spacer elements.
- **Hydration.** `hydrate` resumes server-rendered HTML. Only interactive subtrees
  get listeners.
- **Cleanup.** `MountedRoot::unmount` removes listeners and drops effect handles.
- **Accessibility.** Form controls get stable ids and labels. `DataGrid` has grid
  roles.

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
use std::rc::Rc;

use tpt_appfront_core::UITree;
use wasm_bindgen::prelude::*;

#[derive(Clone)]
enum Msg {
    Increment,
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let document = web_sys::window().unwrap().document().unwrap();
    let body = document.body().unwrap();

    let ui: UITree<Msg> = UITree::container(|c| {
        c.heading(1, "Counter");
        c.button("+1").on_click(Msg::Increment);
    });

    let dispatch = Rc::new(|msg: Msg| match msg {
        Msg::Increment => { /* update state */ }
    });
    tpt_appfront_dom::mount(&body, &ui, dispatch)?;
    Ok(())
}
```

Build with `trunk build` (see [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md)).

## License

MIT OR Apache-2.0
