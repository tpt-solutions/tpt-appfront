# tpt-appfront-canvas

Hardware-accelerated egui/taffy canvas backend for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront) (glow renderer), native and wasm32.

Renders a `UITree<Msg>` immediate-mode each frame via `eframe`/`egui`, laying out with `taffy` and painting with the `glow` (GL/GLES) renderer. Ships `run_native` (desktop windows) and `run_web` (mounts onto a `<canvas>`, wasm32 only).

```toml
[dependencies]
tpt-appfront-core = "0.1"
tpt-appfront-canvas = "0.1"
```

Optional features: `full-text-shaping` (real text shaping via `cosmic-text`) and `accesskit` (screen-reader roles/names). See the [workspace README](https://github.com/tpt-solutions/tpt-appfront#readme) and [docs/quickstart.md](https://github.com/tpt-solutions/tpt-appfront/blob/main/docs/quickstart.md).

## License

Apache-2.0
