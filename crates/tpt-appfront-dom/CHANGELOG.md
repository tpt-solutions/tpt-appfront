# Changelog

All notable changes to `tpt-appfront-dom` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `Textarea`/`Checkbox`/`Select`/`Radio` node kinds now render and reconcile, with
  `on_input`/`on_toggle` wiring (Phase 17).
- `MountedRoot::unmount` drops every tracked listener and `EffectHandle`, so route
  changes / modal closes no longer leak closures.
- `reactive_text` returns `(Node, EffectHandle)` so the caller owns the lifetime;
  `requestAnimationFrame` coalescing of text updates into one flush per frame.
- Streaming hydration (`hydrate`): attaches listeners to server-rendered DOM via
  `data-appfront-id`, islands-only hydration of interactive subtrees.

## [0.1.0]

### Added
- Initial release: `mount`/`render`/`hydrate` over `web-sys`, fine-grained
  in-place updates, keyed `List`/`DataGrid` diffing, `VirtualScroll` windowing,
  and `wasm32`-only compilation (empty crate on native).
