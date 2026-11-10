# tpt-appfront-canvas

The canvas backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

It maps a `UITree` to `egui` widgets. `taffy` computes the layout each frame, and
`eframe` draws the result with the `glow` (GL/GLES) renderer. The same code runs as a
native window and as a `<canvas>` in the browser. Use this crate for GPU-bound or
custom-drawn UIs. For DOM-shaped apps, `tpt-appfront-dom` or `tpt-appfront-webview`
is the better fit.

## Public API

- `run_native(title, build_ui, dispatch) -> eframe::Result<()>` (native targets).
  Opens a window and calls `build_ui` every frame. Clicked `on_click` messages go to
  `dispatch`.
- `run_web(canvas_id, build_ui, dispatch)` (wasm32, async). Mounts onto an existing
  `<canvas>` element and returns `Result<(), JsValue>`.
- `CanvasApp<Msg>`: the `eframe::App` behind both entry points. `CanvasApp::new`,
  `auto_optimize(bool)`, `frame_ms()` and `optimizer_state()`.
- `AutoOptimizer` and `OptimizerState { virtual_scrolling, texture_caching }`.
- `TextMeasurer`: `new()`, `with_font_data(Vec<u8>)` and `measure(text, font_size)`.

Both entry points take `build_ui: impl FnMut() -> UITree<Msg>` and
`dispatch: impl Fn(Msg)`, and `Msg` must be `Clone + 'static`.

## Features

- **Flexbox layout.** `taffy` computes the layout each frame. `winit` and `egui`
  handle input and painting.
- **Text measurement.** The default is a heuristic width estimator. The
  `full-text-shaping` feature enables `cosmic-text` shaping and loads a bundled Noto
  Sans font.
- **Virtual scrolling.** `List` and `DataGrid` nodes with `NodeMeta::virtual_scroll`
  lay out only the visible rows.
- **Utility-class styling.** Padding, font-size and text/background color utilities
  from `class!` apply to layout and paint. The supported set is listed in
  `CANVAS_UTILITIES` in the crate source (not exported).
- **Accessibility.** The `accesskit` feature publishes accessible names and roles for
  painted widgets.
- **`AutoOptimizer`.** Tracks frame time and recommends virtual scrolling and texture
  caching. Call `CanvasApp::auto_optimize(true)` to apply virtual scrolling. Texture
  caching is not applied.

## Features

- `full-text-shaping` (off by default): `cosmic-text` shaping and the bundled font.
- `accesskit` (off by default): AccessKit integration through `eframe`.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-canvas = "0.1"
```

## Example

```rust
use tpt_appfront_core::UITree;

#[derive(Clone)]
enum Msg {
    Increment,
}

fn main() -> eframe::Result<()> {
    let build_ui = || -> UITree<Msg> {
        UITree::container(|c| {
            c.heading(1, "Counter");
            c.button("+1").on_click(Msg::Increment);
        })
    };
    let dispatch = |msg: Msg| match msg {
        Msg::Increment => { /* update state */ }
    };
    tpt_appfront_canvas::run_native("Counter", build_ui, dispatch)
}
```

## License

MIT OR Apache-2.0
