# tpt-appfront-macros

The procedural macros for [TPT AppFront](https://github.com/tpt-solutions/tpt-appfront).

Two macros, re-exported through `tpt-appfront-core` so you typically don't depend on
this crate directly:

- **`#[component]`** (re-exported as `tpt_appfront_core::component`) — wraps a
  `UITree`-returning function and auto-fills `meta.class` (kebab-cased fn name) and
  `meta.ai.description` (from the doc comment) on the root node if you didn't set
  them explicitly. A `memo` mode caches the tree when `PartialEq` props are equal.
- **`view!` / `rsx!`** (re-exported as `tpt_appfront_core::view`) — an HTML-like
  templating macro covering every `NodeKind` (`Container`, `Heading`, `Text`,
  `Button`, `Input`, `Textarea`, `Checkbox`, `Select`, `Radio`, `List`,
  `DataGrid`). Supports `{ expr }` interpolation, two-way binding
  (`on_input`/`on_toggle`), `{if}`/`{for}` control flow, and component-tag
  composition (`{ my_component(props) }`). Statically-known subtrees are hoisted
  into a build-once cache at compile time.

## Install

```toml
[dependencies]
tpt-appfront-core = "0.1"   # re-exports both macros
```

```rust
use tpt_appfront_core::{view, Signal, UITree};

let ui: UITree<Msg> = view! {
    <Container>
        <Heading level={1u8}>"Counter"</Heading>
        <Text>{ format!("Count: {}", count.get()) }</Text>
        <Button on_click={Msg::Increment}>"+1"</Button>
    </Container>
};
```

The macro is purely additive — it expands to the same `UITree::container(|c| { ... })`
builder calls you'd write by hand, so there's no hidden runtime cost.

## License

MIT OR Apache-2.0
