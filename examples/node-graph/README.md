# node-graph

A raw escape-hatch example demonstrating what `tpt-appfront-canvas` *cannot* express
through its `UITree` model.

`tpt-appfront-canvas` renders a generic `UITree<Msg>` through a strictly flexbox
(taffy) layout — no pan/zoom camera, no world-space absolute transforms, no
drag-to-reposition. An infinite, pannable node-graph editor needs exactly those
primitives, so this example bypasses the `UITree` pipeline. It opens the window
with `tpt_appfront_canvas::platform::run`, receives the raw framebuffer each frame,
and paints it with `tpt_appfront_canvas::raster::Canvas` and `glyphs::Font`.

It is kept as the documented escape hatch: when your UI needs free-form
canvas/diagram editing (nodes, wires, pan, zoom, drag), draw into the framebuffer
directly rather than fighting the flexbox layout. If this pattern becomes common,
track a `Canvas`/`Absolute` layout mode in `appfront-core` (see `todo.md` Phase 15)
that could absorb it.

## Run

```sh
cargo run
```

Windows only for now (see `tpt-appfront-canvas`'s platform list). Drag the background
to pan, scroll to zoom (cursor-anchored), drag nodes to reposition, and drag from a
node's right port to another node's left port to create a wire.
