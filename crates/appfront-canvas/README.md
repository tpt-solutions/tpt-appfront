# appfront-canvas

egui/taffy canvas backend (native + WASM) for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Renders an `appfront-core::UITree<Msg>` via `eframe`/`egui`, using `taffy` for CPU-side flexbox-style layout. `CanvasApp` implements `eframe::App`: each frame it rebuilds the `UITree` from your `build_ui` closure (immediate-mode, matching egui's own paradigm), lays it out with a fresh `taffy::TaffyTree`, paints it, and dispatches any clicked `on_click` `Msg` through your `dispatch` callback.

Uses `eframe`'s `glow` (GL/GLES) renderer rather than `wgpu`, deliberately, for a smaller dependency tree and to work on software rasterizers. Two entry points: `run_native` (desktop) and `run_web` (mounts onto a `<canvas>` element, `wasm32` only). Text measurement is pluggable via `TextMeasurer` — a heuristic estimator by default, or real text shaping via the optional `full-text-shaping` feature (`cosmic-text`). An optional `accesskit` feature wires screen-reader names/roles into painted widgets.

See the [project README](https://github.com/tpt-solutions/tpt-appfront#readme) for the full architecture, and `examples/counter-canvas` for a runnable native example.
