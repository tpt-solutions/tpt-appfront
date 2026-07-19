# node-graph

A raw `egui`/`eframe` escape-hatch example demonstrating what
`appfront-canvas` *cannot* currently express.

`appfront-canvas` renders a generic `UITree<Msg>` through a strictly flexbox
(taffy) layout — no pan/zoom camera, no world-space absolute transforms, no
drag-to-reposition. An infinite, pannable node-graph editor needs exactly
those primitives, so this example deliberately bypasses the `appfront-canvas`
pipeline (`run_native`/`CanvasApp`) and drives `eframe` directly.

It is kept as the documented escape hatch: when your UI needs free-form
canvas/diagram editing (nodes, wires, pan, zoom, drag), drop down to raw
`egui`/`eframe` like this rather than fighting the flexbox layout. If this
pattern becomes common, track a `Canvas`/`Absolute` layout mode in
`appfront-core` (see `todo.md` Phase 15) that could absorb it.

## Run

```sh
cargo run
```

Drag the background to pan, scroll to zoom (cursor-anchored), drag nodes to
reposition, and click a node's right port then another node's left port to
create a wire.
