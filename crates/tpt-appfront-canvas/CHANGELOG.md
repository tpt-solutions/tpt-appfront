# Changelog

All notable changes to `tpt-appfront-canvas` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Virtual-scroll windowing for `List`/`DataGrid` (`build_virtual_list`/
  `build_virtual_data_grid`).
- Utility-class (`class!`) styling adapter (`canvas_style_for`) mapping padding /
  font-size / color utilities into taffy layout + egui paint.
- Opt-in `accesskit` feature wiring `egui`/`AccessKit` names for button/input/
  heading/text nodes.
- `AutoOptimizer` per-frame profiling that recommends virtual-scroll / texture-cache
  toggles.

### Changed
- Switched `eframe`/`egui` from the `wgpu` renderer to `glow` (raw GL/GLES),
  cutting native release binary size by ~52% (~7.5 MiB saved).
- Feature-gated `cosmic-text` behind `full-text-shaping` (off by default uses a
  heuristic estimator); the bundled `NotoSans` font is loaded on `wasm32`.

## [0.1.0]

### Added
- Initial release: `run_native` + `CanvasApp` over `eframe`/`winit`/`taffy`,
  `UITree` → `egui` widget mapping, `TextMeasurer`, native + wasm32 support.
