# TPT AppFront

[![CI](https://github.com/tpt-solutions/tpt-appfront/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-appfront/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-APACHE)

Write your UI once in Rust as a plain value, a `UITree<Msg>`, and render it to a browser DOM, a native window, a terminal, semantic HTML for crawlers, or a JSON-LD and AI-agent schema. One codebase, several targets.

```rust
use tpt_appfront::prelude::*;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Increment,
}

fn counter_view(count: i32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, format!("Count: {count}"));
        c.button("Increment").on_click(Msg::Increment);
    })
}
```

Test it without a browser:

```rust
use tpt_appfront::test::Harness;

let count = Signal::new(0i32);
let mut h = Harness::new(counter_view(count.get()));
h.click("Increment");
assert_eq!(h.drain_msgs(), vec![Msg::Increment]);
```

The same `counter_view` renders to the DOM (`tpt-appfront-dom`), to a native window (`tpt-appfront-canvas`), to a terminal (`tpt-appfront-tui`), and to HTML (`tpt-appfront-html`). Other backends can read it too, as described below.

## Documentation

- [Quickstart](docs/quickstart.md): install the CLI, scaffold a project, run it.
- [Cookbook](docs/cookbook.md): forms, routing, stores, async data, SSR and accessibility checks. Every recipe is a passing test.
- [Migration guides](docs/migration/): coming from [React](docs/migration/react.md), [Leptos](docs/migration/leptos.md) or [Dioxus](docs/migration/dioxus.md).
- [Benchmarks](docs/benchmarks.md): what the core operations cost on a reference machine, and what the numbers do not show.
- [Escape hatches](docs/escape-hatches.md): when the `UITree` model does not fit.
- [AI schema](docs/ai-schema.md): the agent-facing output format.
- [Troubleshooting](docs/troubleshooting.md) and [editor setup](docs/editor-setup.md).
- [spec.txt](spec.txt): the full design document. [todo.md](todo.md): build progress.

## Backends

| Backend | Crate | Target | Interactive | Images / media / links | Notes |
|---------|-------|--------|-------------|------------------------|-------|
| Reactive DOM | `tpt-appfront-dom` | browser (wasm32) | yes, all nodes | yes | Mounts once, then updates only changed nodes. Hydrates server output |
| Canvas | `tpt-appfront-canvas` | native and wasm32 | yes, most nodes | captions only | egui with taffy layout. Checkboxes and selects have no focus or key handlers yet |
| Terminal | `tpt-appfront-tui` | any terminal | keyboard only | not focusable | ratatui. Tab and arrows move focus, Enter activates |
| Semantic HTML | `tpt-appfront-html` | server | n/a (static) | yes | SSR and SSG, with `data-ai-*` and OpenGraph tags |
| JSON-LD and AI schema | `tpt-appfront-ai-schema` | server | n/a | n/a | Structured data for search engines and AI agents |
| Desktop webview | `tpt-appfront-webview` | native window | yes | yes | Hosts the DOM bundle in the system webview. Not published to crates.io yet |
| MCP server | `tpt-appfront-mcp` | stdio | agent actions | n/a | Exposes each interactive node as an MCP tool |

Check this table before you rely on a node type on a backend that is not the browser.

## Status

Pre-1.0 (`0.1.x`). The DOM, HTML and server backends are the most exercised. The canvas and TUI backends are usable for simple UIs. Some items are still planned, including a native tray icon, GPU layout, and mobile. [todo.md](todo.md) lists what is done and what is not. The crates have not been published to crates.io yet.

## Quickstart

The CLI is not on crates.io yet, so run it from a checkout:

```sh
git clone https://github.com/tpt-solutions/tpt-appfront && cd tpt-appfront
cargo run -p tpt-appfront-cli -- init my-app --target dom   # or canvas, tui, server
cd my-app && trunk serve                                    # dom: browser
```

See [docs/quickstart.md](docs/quickstart.md) for the full set of commands, presets and options.

## Workspace layout

- `crates/tpt-appfront` — umbrella crate: feature flags for each backend, and a `prelude`
- `crates/tpt-appfront-core` — the `UITree` AST, the `Signal` system, routing, stores, resources, forms, virtual scroll, styling, devtools
- `crates/tpt-appfront-macros` — `view!`, `rsx!` and `#[component]`
- `crates/tpt-appfront-dom` — reactive DOM backend (wasm32)
- `crates/tpt-appfront-canvas` — egui and taffy canvas backend (glow renderer)
- `crates/tpt-appfront-tui` — terminal backend (ratatui)
- `crates/tpt-appfront-html` — semantic HTML backend (SSR and SSG)
- `crates/tpt-appfront-ai-schema` — JSON-LD and AI-agent schema backend
- `crates/tpt-appfront-server` — Axum smart router. Serves the right backend per client (browser, crawler, AI agent, social bot), plus PWA support and a `POST /command` bridge
- `crates/tpt-appfront-mcp` — MCP server exposing the agent API as JSON-RPC tools over stdio
- `crates/tpt-appfront-webview` — desktop shell (wry and tao) hosting the DOM bundle
- `crates/tpt-appfront-templates` — starter UIs (`login_form`, `dashboard_shell`, `settings_list`) as plain tree builders
- `crates/tpt-appfront-test` — headless test harness and accessibility and AI-affordance linter
- `crates/tpt-appfront-cli` — the `tpt-appfront` CLI (`init`, `dev`, `build`, `generate`, `ingest`, `benchmark`, `optimize`, `doctor`, `add`, `fmt`, `lint`, `completions`)

Examples live in `examples/`, outside the workspace, each with its own `Cargo.toml`. They are built in CI. See [docs/quickstart.md](docs/quickstart.md).

When the `UITree` model does not fit, see [docs/escape-hatches.md](docs/escape-hatches.md) for the `{ expr }` composition pattern and the raw-`egui` escape hatch.

## Contributing

This project takes **issues only** for now. Bug reports and feature requests are welcome; pull requests are not being accepted. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
