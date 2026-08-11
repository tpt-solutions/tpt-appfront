# tpt-appfront-core

The shared `UITree` AST and reactive core for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

TPT AppFront lets you write a UI once in Rust as an abstract `UITree<Msg>` and
render it to multiple backends — native/WASM canvas (`tpt-appfront-canvas`),
fine-grained-reactive DOM (`tpt-appfront-dom`), semantic HTML (`tpt-appfront-html`),
AI/JSON-LD schemas (`tpt-appfront-ai-schema`), a terminal UI (`tpt-appfront-tui`),
or an OS-webview desktop shell (`tpt-appfront-webview`) — from one codebase.
This crate has no opinion on rendering: it is the shared AST and reactive system
every backend crate builds on.

## What's inside

- **`UITree<Msg>` / `NodeKind`** — a serializable, backend-agnostic UI tree
  (`Container`, `Heading`, `Text`, `Button`, `Input`, `Textarea`, `Checkbox`,
  `Select`, `Radio`, `List`, `DataGrid`, `Portal`). `Msg` is your app's own
  event enum, so the tree is fully typed.
- **`Signal<T>`** — a `RefCell`-backed reactive value with automatic dependency
  tracking, `create_effect`, memoized derived signals, and `batch()`-coalesced
  updates (diamond-dependency safe).
- **Builder API** — `UITree::container(|c| { c.heading(1, "Hi"); ... })` plus
  `ContainerBuilder`/`NodeRef` chaining (`class`, `key`, `on_click`, `on_input`,
  `on_toggle`, `ai_action`, `attr`, …).
- **`view!` / `#[component]`** — re-exported macros (implemented in
  `tpt-appfront-macros`) for declarative, HTML-like UI and typed components
  (static subtrees are hoisted into a build-once cache at compile time).
- **Rendering helpers** — virtual scrolling (`VirtualScroll`), Tailwind-style
  utility-class styling (`styling`), keyed reconciliation (`reconcile`),
  `Suspense`/`Resource`, routing (`router`), state `Store` with persistence, and
  a pluggable `plugin` system.
- **Devtools / agent API** — `devtools::inspect_tree`, `agent::query_state`,
  `navigate_to`, `AgentState`/`ElementSummary` for headless inspection and
  LLM-driven control.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
```

## Example

```rust
use tpt_appfront_core::{Signal, UITree, view};

#[derive(Debug, Clone)]
enum Msg { Increment }

fn ui(count: Signal<i32>) -> UITree<Msg> {
    view! {
        <Container>
            <Heading level={1u8}>"Counter"</Heading>
            <Text>{ format!("Count: {}", count.get()) }</Text>
            <Button on_click={Msg::Increment}>"+1"</Button>
        </Container>
    }
}
```

## License

MIT OR Apache-2.0
