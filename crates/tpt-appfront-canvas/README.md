# tpt-appfront-canvas

The canvas backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

It maps a `UITree` to widgets painted in software. `taffy` computes the layout each
frame, a built-in painter draws the result into a framebuffer, and a Rust-only window
layer presents it. The crate has no `egui`, `eframe`, or `winit` dependency. Use it for
custom-drawn UIs where a DOM is not the right target. For DOM-shaped apps,
`tpt-appfront-dom` or `tpt-appfront-webview` is the better fit.

## Platforms

- **Windows:** Win32 via `windows-sys`. Run in a real window and exercised by hand.
- **Linux (X11):** the X11 wire protocol through `x11rb-protocol` (pure Rust, no
  libX11). Type-checked for Linux, but not yet run against a live X server. Wayland
  apps run through XWayland. There is no native Wayland backend.
- **macOS:** not yet. `run_native` returns `PlatformError::Unsupported` there.
- **Web (wasm32):** `run_web` paints onto a `<canvas>` and maps DOM mouse, keyboard
  and wheel events. Type-checked for wasm32, but not yet run in a browser.

## Public API

- `run_native(title, build_ui, dispatch) -> Result<(), PlatformError>`: opens an
  800 x 600 window and calls `build_ui` every frame. Messages from clicks, keys, and
  edits go to `dispatch`.
- `run_window(title, width, height, build_ui, dispatch)`: the same with a chosen size.
- `run_web(canvas_id, build_ui, dispatch)` (wasm32, async): paints onto an existing
  `<canvas>` and returns once the animation-frame loop is running.
- `Scene`: the frame-level renderer and input handler behind both. It keeps focus,
  pointer capture, and text-field drafts between frames.
- `platform`: the window layer. `platform::run(title, width, height, |frame| ...)`
  gives each frame a raw framebuffer (`Frame::pixels`, `0x00RRGGBB`) and the input
  events since the last frame. Use it for custom drawing below the `UITree` model.
- `raster`: `Canvas` (rectangles, borders, circles, lines, clipping, text) over a
  framebuffer, plus `rgb`, `Rect` and `Rgb`.
- `glyphs::Font`: text rasterised from the bundled Noto Sans face (OFL-1.1), with a
  per-size glyph cache.
- `TextMeasurer`: `new()` and `measure(text, font_size)`, using the same font as the
  painter so layout and painting agree.

`build_ui` is `impl FnMut() -> UITree<Msg>`, `dispatch` is `impl Fn(Msg)`, and `Msg`
must be `Clone + 'static`.

## Widgets and input

- **Click:** a press and release on the same node dispatches `on_click`. Buttons,
  checkboxes, and other interactive nodes all work this way.
- **Focus:** a click focuses a field or button. Tab and Shift+Tab move through the
  focusable nodes. `on_focus` and `on_blur` fire on each change.
- **Keyboard:** `on_keydown` runs first and can consume a key. Enter and Space activate
  buttons and checkboxes. Printable characters and Backspace edit text fields.
- **Text fields are controlled.** Each edit is sent through `on_input`, and the value
  you render next frame is the source of truth. A short-lived local draft keeps typing
  responsive while your state catches up.
- **Pointer capture:** a node with pointer handlers keeps receiving moves and its
  release even when the pointer leaves it.

## Features

- **Flexbox layout.** `taffy` computes the layout each frame.
- **Text.** Advances are summed from the bundled font, without kerning or shaping.
  Complex scripts that need shaping are not laid out correctly yet.
- **Virtual scrolling.** `List` and `DataGrid` nodes with `NodeMeta::virtual_scroll`
  lay out only the visible rows.
- **Utility-class styling.** Padding, font-size and text/background colour utilities
  from `class!` apply to layout and paint. The supported set is listed in
  `CANVAS_UTILITIES` in the crate source (not exported).
- **`AutoOptimizer`.** Tracks frame time and recommends virtual scrolling and texture
  caching. It is not yet wired into the window loop.

## Not yet supported

- Screen-reader (accessibility) output. The previous `accesskit` feature went with
  egui and is not yet replaced.
- Clipboard paste, mouse-wheel scrolling of lists, and text selection or a cursor
  position inside fields (edits happen at the end of the text).
- macOS has no window backend yet. See **Platforms**.

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

fn main() -> Result<(), tpt_appfront_canvas::platform::PlatformError> {
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
