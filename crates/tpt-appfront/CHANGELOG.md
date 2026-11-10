# Changelog

All notable changes to `tpt-appfront` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial umbrella crate. It re-exports `tpt-appfront-core` as `tpt_appfront::core`
  and adds one feature flag per backend: `dom`, `canvas`, `tui`, `html`, `ai-schema`,
  `server`, `mcp`, `test` and `templates`. No feature is on by default.
- `web` and `full` feature bundles (see the README table). `web` is wasm32-compatible.
- The umbrella's dev-dependencies on `tpt-appfront-test` and `tpt-appfront-html` are path-only,
  so `cargo package` (the release dry run) does not need them on crates.io first.
- `prelude` module with `UITree`, `ContainerBuilder`, `Signal`, `Router`, `Store`,
  `FormState`, and the `view`, `component` and `class` macros.
- `view!`, `rsx!` and `#[component]` resolve their core path through the calling crate,
  so the umbrella alone is enough as a dependency.
