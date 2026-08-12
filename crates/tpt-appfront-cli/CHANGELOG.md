# Changelog

All notable changes to `tpt-appfront-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `init --preset login|dashboard|crud-app|saas-starter` scaffolds a runnable DOM app
  from `tpt-appfront-templates` (+ `view!`) wired to `Signal`-backed state; `--list-presets`
  prints them.
- `dev --devtools` sets `TPT_APPFRONT_DEVTOOLS=1` on the spawned dev process so the
  scaffolded app prints its `UITree` (via `tpt_appfront_core::devtools::inspect_tree`).
- Friendlier tool-not-found errors (`missing_tool_hint`) for `trunk`/cargo-packager, and
  an explicit message when `--desktop-webview` is run without a `ui/index.html`.
- `doctor --a11y`: a deterministic, no-AST heuristic lint over the project's `src/` that
  flags `UITree` builder calls likely missing an accessible name (images with empty `alt`,
  links with empty text, buttons with an empty label). Informational only — never fails the
  run; fits the existing `doctor`/`optimize --analyze` heuristic-scan pattern.

## [0.1.0]

### Added
- Initial release: `init` (canvas/dom/tui/both + `tpt-appfront-templates` presets),
  `dev` (desktop/web/tui/webview, watch/reload loop), `build`/`optimize` (with
  `--bundle` via `cargo packager`), `benchmark`, `doctor`, `generate` (offline
  rule-based `view!`), `ingest` (HTML → `view!`), and `add component`/`add page`
  scaffolding. Path-dep vs published-install resolution via `dep_ref()`.
