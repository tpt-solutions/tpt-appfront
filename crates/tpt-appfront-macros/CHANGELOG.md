# Changelog

All notable changes to `tpt-appfront-macros` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `view!` now supports the `Textarea`/`Checkbox`/`Select`/`Radio` tags (self-closing,
  with `on_input` two-way binding and `on_toggle` for checkboxes), matching the
  existing `Input`/`DataGrid` coverage.
- Static subtrees are hoisted into a build-once cache (`static_tree::static_node`) at
  compile time; `is_dynamic` is now computed precisely from the token tree.

## [0.1.0]

### Added
- Initial release: `#[component]` attribute macro (auto-fills `meta.class` /
  `meta.ai.description`, optional `memo`) and the `view!`/`rsx!` templating macro
  (HTML-like `UITree` builder with `{if}`/`{for}` control flow and component-tag
  composition). Re-exported through `tpt-appfront-core`.
