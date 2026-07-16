# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial public workspace crates: `appfront-core`, `appfront-dom`,
  `appfront-canvas`, `appfront-html`, `appfront-ai-schema`, `appfront-macros`,
  `appfront-cli`, `appfront-server`, `appfront-mcp`, `appfront-tui`,
  `appfront-webview`.

### Notes
- All crates start at `0.1.0` for the first tagged release. Versions are
  managed centrally in the root `Cargo.toml` via `[workspace.package]`.
- After the first tag is cut, bump `[workspace.package] version` here and
  `cargo update -p <crate> --precise <ver>` for any published crates, then
  add a dated section below.

## [0.1.0] - First tagged release

### Added
- Reactive `Signal<T>` core with dependency tracking, `create_memo`, and
  `batch()`-based diamond-dependency dedup (`appfront-core`).
- `UITree` AST (Container/Heading/Text/Button/Input/List/DataGrid) with
  builder API, serde, and virtual-scroll primitive.
- DOM backend (`appfront-dom`): keyed list diffing, fine-grained
  `reactive_text`, rAF-coalesced updates, islands hydration.
- Canvas backend (`appfront-canvas`): egui/glow renderer, taffy layout,
  optional accesskit, optional full-text shaping.
- HTML/SSR backend (`appfront-html`) and AI-schema backend
  (`appfront-ai-schema`) with JSON-LD + custom AI Schema output.
- `appfront-macros`: `#[appfront::component]` and `view!`/`rsx!` templating
  macro with static-subtree hoisting and `class!` compile-time checks.
- `appfront-server`: SmartRouter with ClientKind detection, PWA/offline SW,
  `POST /command`, per-peer rate limiting, security headers.
- `appfront-mcp`: JSON-RPC over stdio MCP server exposing the agent API.
- `appfront-tui`: ratatui terminal backend with keyboard-driven dispatch.
- `appfront-webview`: wry/tao desktop shell with IPC allowlist + rate limiting.
- `appfront-cli`: `init`/`dev`/`build`/`generate`/`benchmark`/`optimize`/`--bundle`.
