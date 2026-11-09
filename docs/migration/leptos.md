# Migrating from Leptos

Leptos is the closest model to AppFront: both have fine-grained signals and a view built once.
The main difference is that Leptos's view is a DOM-producing function, while AppFront's is a
backend-neutral `UITree` value. You gain other render targets. You give up Leptos's compile-time
reactive graph and its server functions.

## Concept map

| Leptos | TPT AppFront |
|--------|--------------|
| `signal(v)` / `create_signal` | `Signal::new(v)`; `.get()`, `.set(v)` |
| `create_effect` | `create_effect` (same idea) |
| `create_memo` | `create_memo` |
| `view! { … }` | `view! { … }` (HTML-like, with `{ expr }` for composition) |
| `on:click=move \|_\| …` | `.on_click(Msg::Variant)`; handled by the app's dispatch |
| `<For each=… key=…>` | `c.list(...)` with `.key(..)` on each item |
| `provide_context` / `use_context` | same names |
| `Resource` / `Suspense` | `Resource` / `Suspense` (`track`, `fallback`) |
| `Form` / server actions | `FormState`; there is no server-function layer (see gotchas) |
| `leptos_router` | `RouteTable` + `Router` |
| `#[component]` | `#[component]` (adds `class` and an AI description from the doc comment) |
| `leptos_axum` SSR | `tpt-appfront-server` (smart router) + `tpt-appfront-html` |

## Side-by-side: a list with selection

Leptos:

```rust
#[component]
fn People(items: Vec<(u32, String)>) -> impl IntoView {
    let (selected, set_selected) = create_signal(None::<u32>);
    view! {
        <ul>
            <For each=move || items.clone() key=|(id, _)| *id
                 children=move |(id, name)| view! {
                    <li on:click=move |_| set_selected.set(Some(id))>{name}</li>
                 }/>
        </ul>
    }
}
```

AppFront. The selection is app state, and clicks produce messages:

```rust
fn people(items: &[(u32, String)]) -> UITree<Msg> {
    UITree::container(|c: &mut ContainerBuilder<Msg>| {
        c.list(|list| {
            for (id, name) in items {
                list.button(name.as_str()).key(id.to_string()).on_click(Msg::Select(*id));
            }
        });
    })
}
```

## Gotchas

- **No server functions.** AppFront has `POST /command` on the smart router, which takes an
  allowlisted action with a JSON body and a rate limit. It is not an RPC layer. If you rely on
  `#[server]` functions, plan the data layer separately and call it from the dispatch handler.
- **Messages, not closures.** Event handlers return a `Msg`, so the component code never captures
  signals in event closures. This is more verbose than `on:click=move |_| …`, but every
  interaction can be tested headlessly.
- **Components are functions returning a tree.** There is no `IntoView` trait and no component
  lifecycle. Use `#[component]` for the metadata it adds, and `{ expr }` in `view!` to compose.
- **Keyed updates are per node.** Use `.key(..)` on list rows, as in Leptos's `<For>`. Without
  keys, reorders look like content changes.
- **Coverage differs.** Leptos targets the browser DOM. AppFront also renders to a native canvas,
  a terminal and HTML for crawlers, so check the backend matrix in the
  [README](../../README.md#backends) before relying on a node type outside the browser.
