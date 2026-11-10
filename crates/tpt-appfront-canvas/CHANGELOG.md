# Changelog

All notable changes to `tpt-appfront-canvas` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
