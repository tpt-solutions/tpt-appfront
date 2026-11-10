# tpt-appfront-core

The shared `UITree` AST and reactive core for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

TPT AppFront lets you write a UI once in Rust as an abstract `UITree<Msg>` and
render it to multiple backends: native/WASM canvas (`tpt-appfront-canvas`),
reactive DOM (`tpt-appfront-dom`), semantic HTML (`tpt-appfront-html`),
AI/JSON-LD schemas (`tpt-appfront-ai-schema`), a terminal UI (`tpt-appfront-tui`),
or an OS-webview desktop shell (`tpt-appfront-webview`). This crate has no
opinion on rendering. It is the shared AST and reactive system every backend
builds on.

## What's inside

- **`UITree<Msg>` / `NodeKind`**: a serde-serializable, backend-agnostic tree.
  Node kinds are `Container`, `Heading`, `Text`, `Button`, `Input`, `Textarea`,
  `Checkbox`, `Select`, `Radio`, `List`, `DataGrid`, `Portal`, `Image`, `Link`,
  and `Media`. `Msg` is your app's own event enum, so the tree is typed.
- **`Signal<T>`**: a single-threaded (`Rc`/`RefCell`) reactive value with
  automatic dependency tracking, `create_effect`, `create_memo`, and `batch()`
  for coalesced, diamond-safe updates.
- **Builder API**: `UITree::container(|c| { c.heading(1, "Hi"); ... })`, plus
  `ContainerBuilder` and `NodeRef` chaining. `NodeRef` methods include `class`,
  `key`, `on_click`, `on_input`, `on_toggle`, `on_focus`, `on_blur`, `on_keydown`,
  `on_pointer_down`/`_move`/`_up`, `on_paste`, `ai_action`, `ai_param`, `attr`, and `aria`.
- **`view!` / `#[component]`**: re-exports of the macros in `tpt-appfront-macros`
  (`view` and its alias `rsx`, and `component`).
- **Rendering helpers**: `VirtualScroll` and `apply_auto_virtual_scroll`,
  Tailwind-style utilities (`styling`, `class!`), keyed reconciliation
  (`reconcile`), `static_tree::static_node`, `Suspense`/`Resource`, routing
  (`Router`, `RouteTable`), a `Store` with optional `Persistence`, forms, error
  boundaries, and a `Plugin` registry.
- **Devtools and agent API**: `devtools::inspect_tree`, `inspect_state`, `render`,
  and `to_html`, plus `agent::query_state`, `navigate_to`, `trigger_event`,
  `AgentState`, and `ElementSummary` for headless inspection and LLM-driven control.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"
```

This crate has no Cargo features.

## Example

```rust
use tpt_appfront_core::{view, Signal, UITree};

#[derive(Debug, Clone)]
enum Msg { Increment }

fn ui(count: &Signal<i32>) -> UITree<Msg> {
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
