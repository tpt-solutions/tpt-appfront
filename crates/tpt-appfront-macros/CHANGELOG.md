# Changelog

All notable changes to `tpt-appfront-macros` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `view!` supports the `Textarea`/`Checkbox`/`Select`/`Radio` tags (self-closing,
  with `on_input` two-way binding and `on_toggle` for checkboxes), matching the
  existing `Input`/`DataGrid` coverage.
- `view!` supports the `Image`, `Link`, and `Media` tags (self-closing, with their
  required attributes: `src`/`alt`, `href`/`text`, and `src`/`alt`/`media_type`). All three
  accept an optional `on_click`.
- `view!` treats a capitalized, self-closing tag that is not a built-in as a custom
  component. `<LoginForm title={..} />` expands to a call into the templates crate,
  resolved as `tpt-appfront-templates` or through the umbrella's `templates` feature.
- Static subtrees are hoisted into a build-once cache (`static_tree::static_node`);
  the hoisting decision is made at compile time.

### Changed
- `is_dynamic` is now computed precisely from the `view!` token tree, instead of
  as a heuristic.

### Fixed
- Expanded code resolves the core crate path through the calling crate's
  dependencies (via `proc-macro-crate`), so crates that depend only on the
  `tpt-appfront` umbrella can use `view!`, `rsx!`, and `#[component]`.
- Custom component tags (`<LoginForm .../>`) resolve `tpt-appfront-templates` the same way.
  They previously named `tpt_appfront_templates` directly, which failed to compile in crates
  that do not depend on it.

## [0.1.0]

### Added
- Initial release: `#[component]` attribute macro (auto-fills `meta.class` /
  `meta.ai.description`, optional `memo`) and the `view!`/`rsx!` templating macro
  (HTML-like `UITree` builder with `{if}`/`{for}` control flow and component-tag
  composition). Re-exported through `tpt-appfront-core`.
