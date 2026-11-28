# Changelog

All notable changes to `tpt-appfront-canvas-3d` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `SceneDelegate` trait: the contract an external 3D engine implements to drive a viewport.
- `Viewport3D<D>`: owns a delegate and a viewport size, and forwards resize, frame, pixel
  readback and pointer hit-testing (`Hit3D`) to it.
- Value types `ViewportPoint`, `ViewportSize`, `Hit3D`, `PresentStatus` and `PixelFrame`.
- No dependencies, so an engine can implement the trait without pulling in a renderer or UI stack.
