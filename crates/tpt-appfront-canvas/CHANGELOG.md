# Changelog

All notable changes to `tpt-appfront-canvas` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed (breaking)
- Replaced `eframe`/`egui` with an internal, Rust-only window layer (`platform`), a
  software painter (`raster`), and bundled-font glyphs (`glyphs`). The crate no longer
  depends on `eframe`, `egui` or `winit`, which were blocked by the Apache-only ban in
  `deny.toml`.
- `run_native` returns `Result<(), PlatformError>` instead of `eframe::Result<()>`.
  It opens an 800 x 600 window. Use `run_window` for another size.
- Window backends: Windows (Win32) and Linux (X11, via the pure-Rust `x11rb-protocol`).
  macOS returns `PlatformError::Unsupported` until its backend lands. The X11 and web
  backends are type-checked but not yet run against a live server or browser.
- `run_web` mounts onto a `<canvas>` again, using `putImageData` and DOM input events
  (wasm32). It previously used `eframe`. It returns once the frame loop is scheduled.
- Text fields are controlled: edits are sent through `on_input` and the value you render
  is the source of truth. Previously edits were not reported.
- Text measurement uses the bundled Noto Sans metrics. The `full-text-shaping` feature
  (`cosmic-text`) is removed, so complex-script shaping is not available yet.
- Removed `CanvasApp`, `CanvasApp::auto_optimize`, `frame_ms`, `optimizer_state`, and
  the `accesskit` feature. Screen-reader output is not yet replaced.
- `TextMeasurer::with_font_data` is removed.

### Added
- Virtual scrolling for `List` and `DataGrid` nodes that set `NodeMeta::virtual_scroll`.
  Only the visible rows are laid out, with spacers for the rest.
- `CanvasApp::auto_optimize(true)`. When the profiler recommends virtual scrolling,
  large `List` and `DataGrid` nodes without a `virtual_scroll` setting are windowed
  automatically.
- Utility-class styling: padding, font-size and text/background color utilities from
  `class!` are applied to taffy layout and egui paint. Unsupported utilities are
  logged at runtime.
- `AutoOptimizer` (re-exported from the crate root) records per-frame time and
  recommends virtual scrolling and texture caching. Texture caching is only a
  recommendation; nothing applies it yet.
- Opt-in `accesskit` feature: painted widgets publish accessible names to AccessKit.
  Headings, links and images also get explicit roles.

### Changed
- Switched `eframe`/`egui` from the `wgpu` renderer to `glow` (raw GL/GLES). On
  Windows, the release build of `examples/counter-canvas` went from about 14.45 MiB
  to about 6.90 MiB (about 52% smaller).
- `cosmic-text` is behind the `full-text-shaping` feature, which is off by default.
  Without it, `TextMeasurer` uses a heuristic width estimator on every target. With
  it, the bundled Noto Sans font is loaded (on wasm32 it is the only font).

## [0.1.0]

### Added
- Initial release: `run_native` + `CanvasApp` over `eframe`/`winit`/`taffy`,
  `UITree` → `egui` widget mapping, `TextMeasurer`, native + wasm32 support.
