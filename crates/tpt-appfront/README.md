# tpt-appfront

The umbrella crate for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).
It re-exports the core `UITree` and signal types, and puts each backend behind a
Cargo feature flag, so one dependency can cover the backends an app uses.

```toml
[dependencies]
tpt-appfront = { version = "0.1", features = ["html", "test"] }
```

```rust
use tpt_appfront::prelude::*;
```

## Feature flags

| Feature | Re-exported as | Backend |
|---------|----------------|---------|
| `dom` | `tpt_appfront::dom` | Reactive DOM (`tpt-appfront-dom`, wasm32 only) |
| `canvas` | `tpt_appfront::canvas` | taffy layout with a software raster (`tpt-appfront-canvas`, native and wasm32) |
| `tui` | `tpt_appfront::tui` | ratatui terminal UI (`tpt-appfront-tui`) |
| `html` | `tpt_appfront::html` | Semantic HTML for SSR and SSG (`tpt-appfront-html`) |
| `ai-schema` | `tpt_appfront::ai_schema` | JSON-LD and AI agent schema (`tpt-appfront-ai-schema`) |
| `server` | `tpt_appfront::server` | Axum smart router (`tpt-appfront-server`) |
| `mcp` | `tpt_appfront::mcp` | MCP server over stdio (`tpt-appfront-mcp`) |
| `test` | `tpt_appfront::test` | Headless test harness and linter (`tpt-appfront-test`) |
| `templates` | `tpt_appfront::templates` | Login, dashboard and settings templates (`tpt-appfront-templates`) |

No feature is on by default. The core types are always available.

Two bundles group the features for common targets:

| Bundle | Enables | Use for |
|--------|---------|---------|
| `web` | `dom`, `html`, `ai-schema`, `templates` | A browser (wasm32) build. It has no native-only dependencies. |
| `full` | `dom`, `canvas`, `html`, `ai-schema`, `server`, `tui`, `mcp`, `templates` | Native targets with every runtime backend. `test` is not included. |

The `webview` backend is not included. Its crate is `publish = false`, and a published
crate cannot depend on an unpublished one. Depend on `tpt-appfront-webview` directly,
by workspace or path dependency.

The per-backend crates (`tpt-appfront-dom`, `tpt-appfront-canvas` and so on) are still
the primary entry points for backend-specific APIs.

## Core re-export

`tpt_appfront::core` is `tpt-appfront-core`, so you can reach any core item that the
prelude does not include.

## Prelude

`use tpt_appfront::prelude::*;` brings in:

- `UITree`, `ContainerBuilder`, `Signal`
- `Router`, `Store`, `FormState`
- the macros `view!`, `component` (the `#[component]` attribute) and `class!`

## Example

```rust
use tpt_appfront::prelude::*;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Clicked,
}

let ui: UITree<Msg> = view! {
    <Container class="p-4">
        <Button on_click={ Msg::Clicked }>"Go"</Button>
    </Container>
};
```

`view!`, `rsx!` and `#[component]` find the core crate through the caller's
dependencies, so depending on this umbrella alone is enough.

## Install

Enable only the features you use. For example, a server that renders HTML and runs
headless tests:

```toml
[dependencies]
tpt-appfront = { version = "0.1", features = ["html", "server", "test"] }
```

## License

MIT OR Apache-2.0
