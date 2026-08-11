# tpt-appfront-canvas

The hardware-accelerated canvas backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

`UITree` → `egui` widgets, laid out with `taffy`, drawn via the `glow` GL/GLES
renderer through `eframe` (winit window). Runs unmodified on desktop (native) and
in the browser (WASM canvas). This is the "beat Electron" desktop path and the
opt-in route for GPU-bound apps (data viz, editors, games) — `tpt-appfront-webview`
remains the default desktop story for DOM-shaped UIs.

## Features

- **Shared layout** via `taffy` (not GPU compute shaders) — flexbox layout per
  frame; `winit`/`egui` handle input and painting.
- **Text shaping** — `full-text-shaping` feature bundles a `NotoSans` font via
  `include_bytes!` so the canvas has text metrics on wasm32 too; a heuristic
  estimator is the default path for minimal size.
- **Virtual scrolling** — `build_virtual_list`/`build_virtual_data_grid` window
  `List`/`DataGrid` items to the visible range + spacers.
- **Utility-class styling** — `canvas_style_for` maps Tailwind-style
  `styling::class!` utilities to taffy layout + egui paint.
- **Accessibility** (opt-in `accesskit` feature) — `egui`/`AccessKit` register
  button/input/heading/text nodes so screen readers see painted-only content.
- **`AutoOptimizer`** — per-frame profiling that recommends toggling virtual
  scrolling / texture caching (profiling + decision half; rendering doesn't yet
  auto-apply the recommendations).

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-canvas = "0.1"
```

## Example

```rust
use tpt_appfront_core::{Signal, UITree};

fn main() -> eframe::Result<()> {
    let count = Signal::new(0i32);
    let build_ui = move || -> UITree<Msg> {
        UITree::container(|c| {
            c.heading(1, "Counter");
            c.button("+1").on_click(Msg::Increment);
        })
    };
    let dispatch = |msg: Msg| { /* ... */ };
    tpt_appfront_canvas::run_native("App", build_ui, dispatch)
}
```

## License

MIT OR Apache-2.0
