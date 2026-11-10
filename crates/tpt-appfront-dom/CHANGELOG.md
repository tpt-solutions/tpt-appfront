# Changelog

All notable changes to `tpt-appfront-dom` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `MountedRoot::render` reconciles a mounted tree against a new `UITree` in place.
  `MountedRoot::unmount` removes every tracked event listener and drops every
  `EffectHandle` the mount owned, so route changes and modal closes don't leak.
- `Textarea`, `Checkbox`, `Select` and `Radio` nodes render and reconcile, wired to
  `on_input` and `on_toggle`.
- `hydrate` attaches listeners to server-rendered DOM matched by `data-appfront-id`.
  Only interactive subtrees are hydrated; static content is left as is.
- Accessibility: form controls get a stable `id` (`af-<data_appfront_id>`).
  Checkbox and Radio are associated with their text through `<label for>`. `Select`
  uses `meta.ai.description` as `aria-label`. `DataGrid` emits `role="grid"` with
  `row`, `columnheader` and `gridcell` roles, and reconciliation reapplies them when
  it recreates rows or headers.
- `render_with` and `mount_router_with` take an `auto_optimize` flag. When it is set,
  large unconfigured lists are windowed automatically.

### Changed
- **Breaking:** `mount` returns `Result<MountedRoot<Msg>, JsValue>` instead of
  `Result<(), JsValue>`, so the caller can keep the handle and call `unmount` later.

## [0.1.0]

### Added
- Initial release: `mount`/`render`/`hydrate` over `web-sys`, fine-grained
  in-place updates, keyed `List`/`DataGrid` diffing, `VirtualScroll` windowing,
  and `wasm32`-only compilation (empty crate on native).
