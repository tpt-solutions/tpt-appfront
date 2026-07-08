# TPT AppFront

A unified, hardware-accelerated, AI-native UI framework: write your UI once in Rust as an abstract `UITree`, render it to native canvas, a fine-grained-reactive DOM, semantic HTML, or machine-readable AI/JSON-LD schemas — from one codebase.

See [spec.txt](spec.txt) for the full design document and [todo.md](todo.md) for build progress.

## Quickstart

_Coming soon — see `docs/quickstart.md` (Phase 8)._

## Workspace layout

- `crates/appfront-core` — `UITree` AST and the reactive `Signal` system
- `crates/appfront-dom` — fine-grained-reactive real DOM backend (web-sys)
- `crates/appfront-canvas` — wgpu/egui hardware-accelerated canvas backend
- `crates/appfront-html` — semantic HTML (SSR/SSG) backend for crawlers
- `crates/appfront-ai-schema` — JSON-LD and AI Schema output backend
- `crates/appfront-macros` — compile-time optimizer/component macros
- `crates/appfront-cli` — the `appfront` CLI

## License

Apache-2.0
