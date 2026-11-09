# Migrating from Dioxus

Dioxus has a virtual DOM, RSX and hooks. AppFront has none of these. It has a backend-neutral
`UITree`, signals and messages. The ports are mostly mechanical for components, and the state
layer needs real rethinking.

## Concept map

| Dioxus | TPT AppFront |
|--------|--------------|
| `rsx! { … }` | `view! { … }` or `rsx! { … }` (same family of macros) |
| `use_signal(|| v)` | `Signal::new(v)` created once, outside the view |
| `use_memo` | `create_memo` |
| `use_effect` | `create_effect` |
| `use_context_provider` / `use_context` | `provide_context` / `use_context` |
| `onclick: move \|_\| …` | `.on_click(Msg::Variant)` |
| `key: "{id}"` | `.key(id)` on the node |
| `Element` / `VNode` return | `UITree<Msg>` return |
| `dioxus-router` | `RouteTable` + `Router` |
| `use_resource` | `Resource` (+ `Suspense` for the loading boundary) |
| `dioxus-ssr` | `tpt_appfront_html::render_page` |
| Desktop (Dioxus desktop / wry) | `tpt-appfront-webview` (wry + tao, hosting the DOM bundle) |
| Dioxus Native / Blitz | `tpt-appfront-canvas` (egui) or `tpt-appfront-tui` |

## Side-by-side: a component with a signal

Dioxus:

```rust
#[component]
fn Counter() -> Element {
    let mut count = use_signal(|| 0);
    rsx! {
        h1 { "Count: {count}" }
        button { onclick: move |_| count += 1, "Increment" }
    }
}
```

AppFront. The signal is created outside the view and the view is built from it. The click
becomes a message:

```rust
#[derive(Debug, Clone, PartialEq)]
enum Msg { Increment }

fn counter_view(count: i32) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.heading(1, format!("Count: {count}"));
        c.button("Increment").on_click(Msg::Increment);
    })
}
```

The app holds `count: Signal<i32>`. When `Msg::Increment` arrives, it calls `count.set(...)`, and
then the view is rebuilt.

## Gotchas

- **Components are not hook-based.** There is no `use_*` call that must run in a fixed order.
  Create signals where the app starts, then pass them down. Nothing about the view's shape depends
  on call order.
- **Events are messages.** Handlers do not mutate signals inline. They return a `Msg`, which keeps
  logic testable (`Harness::click` followed by `drain_msgs`).
- **Desktop is a webview, not a Dioxus renderer.** `tpt-appfront-webview` hosts the same
  `tpt-appfront-dom` WASM bundle as the browser build. If you rely on Dioxus's native renderer,
  choose `tpt-appfront-canvas` or the TUI instead, and check the
  [README](../../README.md#backends) for each one's gaps.
- **The webview crate is not on crates.io.** It is `publish = false`, so use it from a path or git
  dependency for now.
