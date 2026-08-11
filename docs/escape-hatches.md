# Escape Hatches & Composition

TPT AppFront is built around one `UITree<Msg>` AST that every backend
(`tpt-appfront-dom`, `tpt-appfront-canvas`, `tpt-appfront-tui`,
`tpt-appfront-html`, `tpt-appfront-ai-schema`) interprets. Most apps never
need to leave that model — but when you do, there are three clean escape
hatches, ordered from "most idiomatic" to "last resort".

## 1. Compose sub-views with `{ expr }`

Any child slot in `view!` accepts a `{ expr }` that evaluates to a
`UITree<Msg>`. This is the primary composition mechanism: a reusable component
is just a `fn(...) -> UITree<Msg>`, and you embed its result with `{ my_component(props) }`.

```rust
use tpt_appfront_core::{UITree, view};

fn stat_card(title: String, value: String) -> UITree<Msg> {
    view! {
        <Container class="card">
            <Heading level={2u8}>{ title.clone() }</Heading>
            <Text>{ value.clone() }</Text>
        </Container>
    }
}

fn dashboard() -> UITree<Msg> {
    view! {
        <Container>
            { stat_card("Users".into(), "1,204".into()) }
            { stat_card("Revenue".into(), "$9.3k".into()) }
        </Container>
    }
}
```

The `{ expr }` form works both for components you define and for any backend's
own builders. Because the expression is a normal Rust value, you can pass it
through `if`/`for`/function calls freely — see the `view!` control-flow
(`{if}`/`{for}`) and component-tag docs in `docs/quickstart.md`.

## 2. Build by hand with `ContainerBuilder::with`

When you need to assemble a tree procedurally (e.g. fold over a `Vec`), use the
`ContainerBuilder` API directly. `with(child)` appends an already-built
`UITree<Msg>` as a child — it is exactly what `{ expr }` desugars to under the
hood, so the two compose seamlessly.

```rust
use tpt_appfront_core::{ContainerBuilder, UITree};

fn rows(items: &[(String, String)]) -> UITree<Msg> {
    let mut b = ContainerBuilder::new();
    b.container(|c| {
        for (id, label) in items {
            c.with(stat_card(label.clone(), id.clone()));
        }
    });
    b.into_only_child().unwrap()
}
```

`ContainerBuilder::with` is also how the starter templates
(`tpt-appfront-templates`) nest into one another — e.g. `dashboard_shell`'s
`content` field is a `Box<dyn Fn(&mut ContainerBuilder<Msg>)>` that calls
`c.with(...)` to embed a `settings_list`. This is the same composition pattern
the `examples/templates-demo` showcases end to end.

## 3. The raw-backend escape hatch (canvas: `egui`/`eframe`)

`tpt-appfront-canvas` lays out `UITree` nodes with `taffy` and draws them with
`egui`. `taffy`'s flexbox model intentionally cannot express some layouts —
free-form pan/zoom canvases, absolute world-space coordinates, draggable graph
nodes. When you hit that wall, drop below `UITree` and use `egui` directly.

`examples/node-graph/` is the canonical reference: it builds a `CanvasApp`, but
in the `eframe::App::update` body it calls `egui` APIs directly (paint nodes,
handle `response.drag_started()`/`drag_delta()`, `ui.input().scroll_delta`,
etc.) for a zoomable node editor that `taffy` flexbox cannot model. The
`UITree` and the raw `egui` drawing live side by side in the same window.

Rules of thumb for the raw escape hatch:

- Prefer it only for genuinely unsupported layouts (spatial/graph editors,
  custom visualizers). Forms, lists, and dashboards belong in `UITree`.
- Keep the raw `egui` code confined to a single module so the rest of the app
  stays backend-agnostic.
- You lose automatic cross-backend rendering (the raw code only runs on
  canvas), so gate it behind a `cfg` or a clearly-named module if you also ship
  a DOM build.

For DOM and TUI there is no equivalent raw escape hatch yet — those backends
are intentionally "just" interpreters of `UITree`. If you need bespoke DOM
behavior, contribute it through `NodeKind`/a custom component rather than
bypassing the tree.

## When to use which

| Need | Use |
| --- | --- |
| Reuse a piece of UI | `{ expr }` → component fn |
| Fold/build procedurally | `ContainerBuilder::with` |
| Nest starter templates | templates' `content`/`c.with(...)` |
| Layout `taffy` can't express (canvas) | raw `egui` in `examples/node-graph` style |
