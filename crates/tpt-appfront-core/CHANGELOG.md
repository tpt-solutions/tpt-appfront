# Changelog

All notable changes to `tpt-appfront-core` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `ContainerBuilder` now exposes `textarea`, `checkbox`, `select`, and
  `radio_group` node builders (consumed by the `view!` macro's
  `Textarea`/`Checkbox`/`Select`/`Radio` tags).
- `devtools::inspect_tree` / `render` / `to_html` for headless tree inspection
  (also surfaced by the CLI's `dev --devtools` flag).
- `styling` utility-class system (`lookup`, `class!` macro) and `virtual_scroll`
  primitive.

### Changed
- `on_toggle` (checkbox) and `on_input` (input/textarea/select/radio) are now
  `Fn(...) -> Msg` closures, enabling two-way binding.

## [0.1.0]

### Added
- Initial release: `UITree<Msg>` AST and `NodeKind` set, `Signal<T>` reactive
  system with `create_effect`/`batch`/memoized signals, builder API
  (`UITree::container`, `ContainerBuilder`, `NodeRef`), `view!`/`#[component]`
  macro re-exports, routing, `Store` persistence, `Suspense`/`Resource`,
  `AgentState`/`ElementSummary` agent API, static-tree caching, and keyed
  reconciliation primitives.
